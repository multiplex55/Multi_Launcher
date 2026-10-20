#requires -Version 7.0
[CmdletBinding()]
param(
    [switch]$FixturesOnly,
    [ValidateSet('Baseline', 'QueryProbe', 'Phase1', 'Phase1Enabled', 'Phase1Disabled')][string]$Scenario = 'Baseline',
    [ValidateSet('All', 'Dashboard', 'Grid')][string]$Screen = 'All',
    [ValidateRange(5, 120)][int]$CaptureTimeoutSeconds = 45,
    [ValidateRange(1, 30)][int]$GracefulCloseSeconds = 6,
    [ValidateRange(300, 7200)][int]$OverallTimeoutSeconds = 2400
)

$ErrorActionPreference = 'Stop'
$repoRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))
$cargoFeatures = 'dev-smoke-binaries,qa-framebuffer-capture'
$runStamp = [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssZ')
$runSuffix = [Guid]::NewGuid().ToString('N').Substring(0, 8)
$scenarioTag = switch ($Scenario) {
    'QueryProbe' { 'm05_query_probe' }
    'Phase1' { 'm08_phase1' }
    'Phase1Enabled' { 'm08_phase1_enabled' }
    'Phase1Disabled' { 'm08_phase1_disabled' }
    default { 'm01_baseline' }
}
$runId = "jump_${scenarioTag}_${runStamp}_$runSuffix"
if ($Scenario -eq 'Phase1Enabled' -and $Screen -ne 'All') { $runId = "${runId}_$($Screen.ToLowerInvariant())" }
$evidenceRoot = Join-Path $repoRoot "dev\jump_mode\evidence\$runId"
$profileRoot = Join-Path $evidenceRoot 'profile'
$captureRoot = Join-Path $evidenceRoot 'capture'
$screenshotsRoot = Join-Path $evidenceRoot 'screenshots'
$eventsPath = Join-Path $evidenceRoot 'events.jsonl'
$summaryPath = Join-Path $evidenceRoot 'summary.md'
$manifestPath = Join-Path $evidenceRoot 'manifest.json'
$scenariosPath = Join-Path $evidenceRoot 'scenarios.json'
$lifecyclePath = Join-Path $evidenceRoot 'process_lifecycle.json'
$stdoutPath = Join-Path $evidenceRoot 'stdout.log'
$stderrPath = Join-Path $evidenceRoot 'stderr.log'
$fixtureStdoutPath = Join-Path $evidenceRoot 'fixture.stdout.log'
$fixtureStderrPath = Join-Path $evidenceRoot 'fixture.stderr.log'
$testStdoutPath = Join-Path $evidenceRoot 'fixture-tests.stdout.log'
$testStderrPath = Join-Path $evidenceRoot 'fixture-tests.stderr.log'
$buildStdoutPath = Join-Path $evidenceRoot 'build.stdout.log'
$buildStderrPath = Join-Path $evidenceRoot 'build.stderr.log'
$startedAt = [DateTime]::UtcNow
$runDeadlineUtc = $startedAt.AddSeconds($OverallTimeoutSeconds)
$targetDirectory = $null
$previousTargetDirectory = $env:CARGO_TARGET_DIR
$process = $null
$stdoutTask = $null
$stderrTask = $null
$processStartUtc = $null
$processId = $null
$processSessionId = $null
$fixtureHash = $null
$executablePath = $null
$executableHash = $null
$buildReceiptIdentity = $null
$captureReceipt = $null
$screenshotPath = $null
$screenshotWidth = $null
$screenshotHeight = $null
$gracefulCloseRequested = $false
$forcedTreeCleanup = $false
$processExitCode = $null
$postCleanupState = 'not_started'
$ownedTreeSnapshot = @()
$ownedTreeVerified = $false
$scenarioStatus = 'BLOCKED'
$scenarioReason = switch ($Scenario) {
    'QueryProbe' { 'Native Jump Mode query probe has not started.' }
    { $_ -in @('Phase1', 'Phase1Enabled', 'Phase1Disabled') } { 'Native Phase 1 Jump Mode scenarios have not started.' }
    default { 'Baseline capture has not started.' }
}
$fixtureScenarioStatus = 'BLOCKED'
$fixtureScenarioReason = 'Fixture validation has not started.'
$cleanupIssues = [Collections.Generic.List[string]]::new()
$runStatus = 'BLOCKED'
$phase = 'initializing'
$failureText = $null
$interactive = [Environment]::UserInteractive
$sessionId = [Diagnostics.Process]::GetCurrentProcess().SessionId
$foregroundActivationAttempted = $false
$jumpObservationPath = Join-Path $captureRoot 'jump-observation.json'
$phase1Captures = [Collections.Generic.List[object]]::new()
$phase1CaptureCount = if ($Scenario -eq 'Phase1Enabled') {
    switch ($Screen) { 'Dashboard' { 5 } 'Grid' { 9 } default { 14 } }
} elseif ($Scenario -eq 'Phase1Disabled') { 1 } else { 0 }
$phase1BaseActivationCount = 1
$appWindowMetadata = $null

function Get-RemainingRunSeconds([int]$RequestedSeconds = 86400) {
    $remaining = [int][Math]::Floor(($script:runDeadlineUtc - [DateTime]::UtcNow).TotalSeconds)
    if ($remaining -le 0) { throw "overall runner deadline of $OverallTimeoutSeconds seconds expired" }
    return [Math]::Min($RequestedSeconds, $remaining)
}

function Get-RunDeadline([int]$RequestedSeconds) {
    return [DateTime]::UtcNow.AddSeconds((Get-RemainingRunSeconds $RequestedSeconds))
}

function Invoke-BoundedCommand([string]$FilePath, [string[]]$Arguments, [string]$WorkingDirectory, [string]$StdoutPath, [string]$StderrPath, [string]$Description) {
    $null = Get-RemainingRunSeconds $OverallTimeoutSeconds
    $startInfo = [Diagnostics.ProcessStartInfo]::new()
    $startInfo.FileName = $FilePath
    $startInfo.WorkingDirectory = $WorkingDirectory
    $startInfo.UseShellExecute = $false
    $startInfo.CreateNoWindow = $true
    $startInfo.RedirectStandardOutput = $true
    $startInfo.RedirectStandardError = $true
    foreach ($argument in $Arguments) { $startInfo.ArgumentList.Add($argument) }
    $commandProcess = [Diagnostics.Process]::new()
    $commandProcess.StartInfo = $startInfo
    $stdoutTask = $null
    $stderrTask = $null
    $logsWritten = $false
    try {
        if (!$commandProcess.Start()) { throw "failed to start $Description" }
        $commandPid = $commandProcess.Id
        $commandStartUtc = $commandProcess.StartTime.ToUniversalTime()
        $stdoutTask = $commandProcess.StandardOutput.ReadToEndAsync()
        $stderrTask = $commandProcess.StandardError.ReadToEndAsync()
        while (!$commandProcess.WaitForExit(100)) {
            if ([DateTime]::UtcNow -ge $runDeadlineUtc) {
                $killRequested = $false
                $rootExitVerified = $false
                $terminationError = $null
                try {
                    $commandProcess.Refresh()
                    $sameProcess = $commandProcess.Id -eq $commandPid -and
                        [Math]::Abs(($commandProcess.StartTime.ToUniversalTime() - $commandStartUtc).TotalMilliseconds) -le 5
                    if ($sameProcess) {
                        if (!$commandProcess.HasExited) {
                            $commandProcess.Kill($true)
                            $killRequested = $true
                            $null = $commandProcess.WaitForExit(5000)
                        }
                        $commandProcess.Refresh()
                        $rootExitVerified = $commandProcess.HasExited
                    } else {
                        $terminationError = 'retained PID/start identity no longer matches'
                    }
                } catch { $terminationError = $_.Exception.Message }
                throw "overall runner deadline of $OverallTimeoutSeconds seconds expired during $Description; process-tree kill requested=$killRequested; owned command root exit verified=$rootExitVerified; descendant and owned-app cleanup are not independently verified. $terminationError"
            }
        }
        $commandProcess.WaitForExit()
        $standardOutput = $stdoutTask.GetAwaiter().GetResult()
        $standardError = $stderrTask.GetAwaiter().GetResult()
        [IO.File]::WriteAllText($StdoutPath, $standardOutput, [Text.UTF8Encoding]::new($false))
        [IO.File]::WriteAllText($StderrPath, $standardError, [Text.UTF8Encoding]::new($false))
        $logsWritten = $true
        return [pscustomobject]@{ ExitCode = $commandProcess.ExitCode; StandardOutput = $standardOutput; StandardError = $standardError }
    } finally {
        if (!$logsWritten) {
            $standardOutput = if ($stdoutTask -and $stdoutTask.IsCompleted) { $stdoutTask.GetAwaiter().GetResult() } else { '[stdout capture incomplete because the bounded command did not finish]' }
            $standardError = if ($stderrTask -and $stderrTask.IsCompleted) { $stderrTask.GetAwaiter().GetResult() } else { '[stderr capture incomplete because the bounded command did not finish]' }
            try { [IO.File]::WriteAllText($StdoutPath, $standardOutput, [Text.UTF8Encoding]::new($false)) } catch { }
            try { [IO.File]::WriteAllText($StderrPath, $standardError, [Text.UTF8Encoding]::new($false)) } catch { }
        }
        try { $commandProcess.Dispose() } catch { }
    }
}
$phase1NextCaptureIndex = 1
$phase1Orchestration = $null

function Write-Json([string]$Path, $Value) {
    $json = ConvertTo-Json -InputObject $Value -Depth 32
    [IO.File]::WriteAllText($Path, $json + [Environment]::NewLine, [Text.UTF8Encoding]::new($false))
}

function Write-Event([string]$Name, [hashtable]$Data = @{}) {
    $event = [ordered]@{
        at_utc = [DateTime]::UtcNow.ToString('O')
        event = $Name
        data = $Data
    }
    $line = ConvertTo-Json -InputObject $event -Compress -Depth 20
    [IO.File]::AppendAllText($eventsPath, $line + [Environment]::NewLine, [Text.UTF8Encoding]::new($false))
}

function Write-CleanupEvent([string]$Name, [hashtable]$Data = @{}) {
    try { Write-Event $Name $Data } catch { }
}

function Record-CleanupIssue([string]$Step, [string]$Message) {
    $description = "$Step`: $Message"
    $script:cleanupIssues.Add($description)
    if (!$script:failureText) { $script:failureText = $description }
    $script:postCleanupState = 'cleanup_uncertain'
    Write-CleanupEvent 'cleanup_error' @{ pid = $script:processId; step = $Step; message = $Message }
}

function Write-Summary([string]$Status, [string]$Details) {
    $lines = @(
        "# Jump Mode $Scenario run $runId",
        '',
        "Status: **$Status**",
        '',
        "Started (UTC): $($startedAt.ToString('O'))",
        "Updated (UTC): $([DateTime]::UtcNow.ToString('O'))",
        "Interactive session: $interactive (session $sessionId)",
        "Fixture profile: ``$profileRoot``",
        "Build: ``cargo build --profile fast-dev --features $cargoFeatures --bin multi_launcher``",
        '',
        $Details,
        ''
    )
    if ($screenshotPath) {
        $lines += "Screenshot: ``$screenshotPath`` ($screenshotWidth x $screenshotHeight PNG pixels)"
    }
    if ($processId) {
        $lines += "Owned process: PID $processId started $($processStartUtc.ToString('O')); cleanup state: $postCleanupState."
    }
    if ($failureText) {
        $lines += "Failure detail: $failureText"
    }
    $lines += ''
    [IO.File]::WriteAllLines($summaryPath, $lines, [Text.UTF8Encoding]::new($false))
}

function Get-PngDimensions([string]$Path) {
    $bytes = [IO.File]::ReadAllBytes($Path)
    $signature = [byte[]](137, 80, 78, 71, 13, 10, 26, 10)
    if ($bytes.Length -lt 24) { throw 'captured PNG is shorter than its required header' }
    for ($index = 0; $index -lt $signature.Length; $index++) {
        if ($bytes[$index] -ne $signature[$index]) { throw 'captured screenshot is not a PNG file' }
    }
    if ([Text.Encoding]::ASCII.GetString($bytes, 12, 4) -ne 'IHDR') { throw 'captured PNG has no IHDR header' }
    $width = ([uint32]$bytes[16] -shl 24) -bor ([uint32]$bytes[17] -shl 16) -bor ([uint32]$bytes[18] -shl 8) -bor [uint32]$bytes[19]
    $height = ([uint32]$bytes[20] -shl 24) -bor ([uint32]$bytes[21] -shl 16) -bor ([uint32]$bytes[22] -shl 8) -bor [uint32]$bytes[23]
    if ($width -eq 0 -or $height -eq 0) { throw 'captured PNG has empty dimensions' }
    return [pscustomobject]@{ Width = [int]$width; Height = [int]$height; Bytes = $bytes.Length }
}

function Initialize-NativeQueryProbe {
    $nativeType = @'
using System;
using System.Runtime.InteropServices;

public static class JumpModeNativeProbe {
    [StructLayout(LayoutKind.Sequential)]
    private struct KeyboardInput {
        public ushort virtualKey;
        public ushort scanCode;
        public uint flags;
        public uint time;
        public UIntPtr extraInfo;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct MouseInput {
        public int dx;
        public int dy;
        public uint mouseData;
        public uint flags;
        public uint time;
        public UIntPtr extraInfo;
    }

    [StructLayout(LayoutKind.Explicit, Size = 32)]
    private struct InputUnion {
        [FieldOffset(0)] public KeyboardInput keyboard;
        [FieldOffset(0)] public MouseInput mouse;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct Input {
        public uint type;
        public InputUnion data;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct Rect {
        public int left;
        public int top;
        public int right;
        public int bottom;
    }

    [DllImport("user32.dll", SetLastError = true)]
    private static extern uint SendInput(uint count, [In] Input[] inputs, int size);

    [DllImport("user32.dll")]
    private static extern IntPtr GetForegroundWindow();

    [DllImport("user32.dll")]
    private static extern uint GetWindowThreadProcessId(IntPtr window, out uint processId);

    [DllImport("user32.dll")]
    private static extern bool SetForegroundWindow(IntPtr window);

    [DllImport("user32.dll")]
    private static extern short GetAsyncKeyState(int key);

    [DllImport("user32.dll", SetLastError = true)]
    private static extern bool GetWindowRect(IntPtr window, out Rect rect);

    [DllImport("user32.dll", SetLastError = true)]
    private static extern uint GetDpiForWindow(IntPtr window);

    public static uint ForegroundProcessId() {
        IntPtr window = GetForegroundWindow();
        if (window == IntPtr.Zero) return 0;
        uint processId;
        GetWindowThreadProcessId(window, out processId);
        return processId;
    }

    public static bool ActivateWindow(IntPtr window) {
        return window != IntPtr.Zero && SetForegroundWindow(window);
    }

    public static bool ModifierIsDown() {
        int[] modifiers = { 0x10, 0x11, 0x12, 0x5B, 0x5C };
        foreach (int key in modifiers) {
            if ((GetAsyncKeyState(key) & 0x8000) != 0) return true;
        }
        return false;
    }

    public static bool SendKey(ushort virtualKey, bool keyUp) {
        Input input = new Input();
        input.type = 1;
        input.data.keyboard.virtualKey = virtualKey;
        input.data.keyboard.flags = keyUp ? 0x0002u : 0u;
        Input[] inputs = { input };
        return SendInput(1, inputs, Marshal.SizeOf(typeof(Input))) == 1;
    }

    public static bool TryGetWindowBounds(IntPtr window, out int left, out int top, out int right, out int bottom) {
        Rect rect;
        bool success = GetWindowRect(window, out rect);
        left = rect.left;
        top = rect.top;
        right = rect.right;
        bottom = rect.bottom;
        return success;
    }

    public static uint WindowDpi(IntPtr window) {
        return GetDpiForWindow(window);
    }
}
'@
    Add-Type -TypeDefinition $nativeType -Language CSharp -ErrorAction Stop
}

function Get-OwnedWindowMetadata {
    $process.Refresh()
    $window = $process.MainWindowHandle
    if ($window -eq [IntPtr]::Zero) { throw 'owned app main window is unavailable for metadata capture' }
    $left = 0; $top = 0; $right = 0; $bottom = 0
    $hasBounds = [JumpModeNativeProbe]::TryGetWindowBounds($window, [ref]$left, [ref]$top, [ref]$right, [ref]$bottom)
    $windowDpi = $null
    try {
        $observedDpi = [JumpModeNativeProbe]::WindowDpi($window)
        if ($observedDpi -gt 0) { $windowDpi = [int]$observedDpi }
    } catch { }
    return [ordered]@{
        hwnd = ('0x{0:X}' -f $window.ToInt64())
        bounds = if ($hasBounds) { [ordered]@{ left = $left; top = $top; right = $right; bottom = $bottom; width = $right - $left; height = $bottom - $top } } else { $null }
        dpi = $windowDpi
    }
}

function Read-JumpModeObservation {
    if (!(Test-Path -LiteralPath $jumpObservationPath -PathType Leaf)) { return $null }
    try {
        $json = Get-Content -Raw -LiteralPath $jumpObservationPath
        if ([string]::IsNullOrWhiteSpace($json)) { return $null }
        $observation = $json | ConvertFrom-Json
    } catch {
        return $null
    }
    if ($null -eq $observation -or $null -eq $observation.process_pid -or
        [string]::IsNullOrWhiteSpace([string]$observation.fixture_manifest_sha256)) { return $null }
    if ([int]$observation.process_pid -ne [int]$script:processId) {
        throw "Jump Mode observation PID $($observation.process_pid) does not match retained app PID $script:processId"
    }
    if ([string]$observation.fixture_manifest_sha256 -cne [string]$script:fixtureHash) {
        throw 'Jump Mode observation fixture hash does not match the verified isolated fixture'
    }
    return $observation
}

function Wait-JumpModeObservation([scriptblock]$Condition, [int]$TimeoutSeconds, [string]$Description) {
    $boundedTimeout = Get-RemainingRunSeconds $TimeoutSeconds
    $deadline = [DateTime]::UtcNow.AddSeconds($boundedTimeout)
    while ([DateTime]::UtcNow -lt $deadline) {
        $process.Refresh()
        if ($process.HasExited) {
            $script:processExitCode = $process.ExitCode
            throw "isolated app exited while waiting for $Description (exit $script:processExitCode)"
        }
        $observation = Read-JumpModeObservation
        if ($null -ne $observation -and (& $Condition $observation)) { return $observation }
        Start-Sleep -Milliseconds 100
    }
    throw "timed out after $boundedTimeout seconds waiting for $Description"
}

function Test-Phase1CapturePredicateComposition {
    $testPath = Join-Path $evidenceRoot 'predicate-composition-observation.json'
    $savedPath = $script:jumpObservationPath
    $savedProcess = $script:process
    $savedProcessId = $script:processId
    $savedFixtureHash = $script:fixtureHash
    try {
        $testProcess = [Diagnostics.Process]::GetCurrentProcess()
        $testFixtureHash = 'a' * 64
        $script:jumpObservationPath = $testPath
        $script:process = $testProcess
        $script:processId = $testProcess.Id
        $script:fixtureHash = $testFixtureHash
        $expectedIndex = [Math]::Min(2, $phase1CaptureCount)
        Write-Json $testPath ([ordered]@{
            process_pid = $testProcess.Id
            fixture_manifest_sha256 = $testFixtureHash
            capture_armed = $true
            capture_index = $expectedIndex
            capture_count = $phase1CaptureCount
            query = 'note list'
            activation_count = 0
        })
        $Predicate = { param($state) $state.query -ceq 'note list' -and $state.activation_count -eq 0 }
        $captureCondition = {
            param($state)
            $state.capture_armed -and $state.capture_index -eq $expectedIndex -and
                $state.capture_count -eq $phase1CaptureCount -and (& $Predicate $state)
        }
        $matched = Wait-JumpModeObservation $captureCondition 1 'headless capture predicate composition regression'
        if (!$matched.capture_armed -or $matched.query -cne 'note list' -or $matched.activation_count -ne 0) {
            throw 'capture predicate composition returned a mismatched observation'
        }
        [IO.File]::WriteAllText($testPath, '')
        if ($null -ne (Read-JumpModeObservation)) {
            throw 'empty in-progress observation file was not treated as a pending frame'
        }
        Write-Json $testPath ([ordered]@{
            process_pid = $testProcess.Id + 1
            fixture_manifest_sha256 = $testFixtureHash
            capture_armed = $true
            capture_index = $expectedIndex
            capture_count = $phase1CaptureCount
            query = 'note list'
            activation_count = 0
        })
        $wrongPidRejected = $false
        try {
            $null = Read-JumpModeObservation
        } catch {
            $wrongPidRejected = $_.Exception.Message -like '*does not match retained app PID*'
        }
        if (!$wrongPidRejected) { throw 'complete observation with a mismatched retained PID was not rejected' }
        Write-Event 'capture_predicate_composition_regression_passed' @{ capture_index = $expectedIndex; capture_count = $phase1CaptureCount; query = $matched.query; activation_count = $matched.activation_count }
    } finally {
        $script:jumpObservationPath = $savedPath
        $script:process = $savedProcess
        $script:processId = $savedProcessId
        $script:fixtureHash = $savedFixtureHash
        Remove-Item -LiteralPath $testPath -Force -ErrorAction SilentlyContinue
    }
}

function Get-ObservedQueryTarget($Observation) {
    $queryCandidates = @($Observation.targets | Where-Object {
        $_.is_query_input -and $_.kind -eq 'text_entry' -and $_.eligible
    })
    if ($queryCandidates.Count -ne 1) { throw "Jump Mode observation has $($queryCandidates.Count) eligible query TextEntry targets" }
    if (!$queryCandidates[0].hint -or [string]$queryCandidates[0].hint -notmatch '^[A-Z]+$') {
        throw 'the observed query target has no valid active Jump Mode hint'
    }
    return $queryCandidates[0]
}

function Test-ObservedQueryTargetLookup {
    $expected = [pscustomobject]@{
        target_id = 'fixture-query-target'
        hint = 'AA'
        is_query_input = $true
        kind = 'text_entry'
        eligible = $true
    }
    $target = Get-ObservedQueryTarget ([pscustomobject]@{ targets = @($expected) })
    if ($target.target_id -cne $expected.target_id -or $target.hint -cne $expected.hint) {
        throw 'observed query target lookup did not preserve the exact target ID and hint'
    }
}

function Assert-OwnedForeground {
    if (!(Test-RetainedProcessIdentity)) { throw 'retained app process identity changed before native keyboard input' }
    $foregroundPid = [JumpModeNativeProbe]::ForegroundProcessId()
    if ($foregroundPid -ne $processId) {
        if ($script:foregroundActivationAttempted) {
            throw "foreground input owner changed to PID $foregroundPid; refusing native keyboard input"
        }
        $windowDeadline = Get-RunDeadline ([Math]::Min($CaptureTimeoutSeconds, 15))
        $window = [IntPtr]::Zero
        while ([DateTime]::UtcNow -lt $windowDeadline -and $window -eq [IntPtr]::Zero) {
            if (!(Test-RetainedProcessIdentity)) { throw 'retained app process identity changed while locating its window' }
            $process.Refresh()
            if ($process.HasExited) {
                $script:processExitCode = $process.ExitCode
                throw "isolated app exited before creating its main window (exit $script:processExitCode)"
            }
            $window = $process.MainWindowHandle
            if ($window -eq [IntPtr]::Zero) { Start-Sleep -Milliseconds 100 }
        }
        if ($window -eq [IntPtr]::Zero) { throw "the retained app process did not create a main window within $([Math]::Min($CaptureTimeoutSeconds, 15)) seconds" }
        $script:foregroundActivationAttempted = $true
        $null = [JumpModeNativeProbe]::ActivateWindow($window)
        $foregroundDeadline = Get-RunDeadline 3
        do {
            if ([JumpModeNativeProbe]::ForegroundProcessId() -eq $processId) { break }
            Start-Sleep -Milliseconds 100
        } while ([DateTime]::UtcNow -lt $foregroundDeadline)
        $foregroundPid = [JumpModeNativeProbe]::ForegroundProcessId()
        if ($foregroundPid -ne $processId) { throw "could not focus the owned app window; foreground PID is $foregroundPid" }
    }
    if ([JumpModeNativeProbe]::ModifierIsDown()) {
        throw 'a Ctrl, Alt, Shift, or Windows key was already held; refusing native keyboard input'
    }
}

function Invoke-NativeKeyBatch([string]$Name, [uint16[]]$Keys, [string[]]$KeyLabels) {
    if ($Keys.Count -ne $KeyLabels.Count -or $Keys.Count -eq 0) { throw 'native key batch is empty or malformed' }
    Assert-OwnedForeground
    $pressed = [Collections.Generic.List[uint16]]::new()
    $sendFailure = $null
    try {
        foreach ($key in $Keys) {
            if (![JumpModeNativeProbe]::SendKey($key, $false)) { throw "SendInput key-down failed in $Name" }
            $pressed.Add($key)
        }
    } catch {
        $sendFailure = $_.Exception.Message
    } finally {
        for ($index = $pressed.Count - 1; $index -ge 0; $index--) {
            if (![JumpModeNativeProbe]::SendKey($pressed[$index], $true)) {
                $sendFailure = "SendInput key-up failed while releasing keys from $Name"
            }
        }
    }
    if ($sendFailure) { throw $sendFailure }
    Write-Event 'native_key_batch_sent' @{ name = $Name; keys = $KeyLabels; count = $Keys.Count; foreground_pid = $processId }
}

function Get-Phase1ResultTargets($Observation) {
    return @($Observation.targets | Where-Object {
        $_.surface -eq 'launcher_results' -and $_.eligible -and $null -ne $_.root_action
    })
}

function Get-Phase1QueryTarget($Observation) {
    $queryTargets = @($Observation.targets | Where-Object {
        $_.surface -eq 'launcher_results' -and $_.is_query_input -and
            $_.kind -eq 'text_entry' -and $_.eligible -and $_.hint -match '^[A-Z]+$'
    })
    if ($queryTargets.Count -ne 1) { throw "Phase 1 observed $($queryTargets.Count) eligible query targets" }
    return $queryTargets[0]
}

function Get-Phase1Target($Observation, [scriptblock]$Predicate, [string]$Description) {
    $candidateTargets = @($Observation.targets | Where-Object {
        $_.eligible -and $_.hint -match '^[A-Z]+$' -and (& $Predicate $_)
    })
    if ($candidateTargets.Count -eq 0) { throw "no eligible hinted target matched $Description" }
    return $candidateTargets[0]
}

function Assert-Phase1PrefixFree($Targets, [string]$Description) {
    $hints = @($Targets | ForEach-Object { [string]$_.hint })
    $distinctHints = @($hints | Sort-Object -Unique)
    if ($distinctHints.Count -ne $hints.Count) { throw "$Description contains duplicate hints" }
    for ($left = 0; $left -lt $distinctHints.Count; $left++) {
        for ($right = $left + 1; $right -lt $distinctHints.Count; $right++) {
            if ($distinctHints[$left].StartsWith($distinctHints[$right], [StringComparison]::Ordinal) -or
                $distinctHints[$right].StartsWith($distinctHints[$left], [StringComparison]::Ordinal)) {
                throw "$Description is not prefix-free: $($distinctHints[$left]) and $($distinctHints[$right])"
            }
        }
    }
}

function Send-ObservedHint([string]$Name, [string]$Hint) {
    if (!$Hint -or $Hint -notmatch '^[A-Z]+$') { throw "refusing to send invalid observed hint $Hint" }
    $index = 0
    foreach ($character in $Hint.ToCharArray()) {
        $index++
        Invoke-NativeKeyBatch "$Name-$index" ([uint16[]]@([char]$character)) ([string[]]@([string]$character))
    }
}

function Send-NativeText([string]$Name, [string]$Text) {
    $index = 0
    foreach ($character in $Text.ToCharArray()) {
        $index++
        $key = if ($character -eq ' ') { [uint16]0x20 } else { [uint16][char]([char]::ToUpperInvariant($character)) }
        Invoke-NativeKeyBatch "$Name-$index" ([uint16[]]@($key)) ([string[]]@([string]$character))
    }
}

function Ensure-Phase1QueryFocus([string]$Description, [bool]$RequireJumpRootOwner = $true) {
    $state = Wait-JumpModeObservation {
        param($candidate)
        $candidate.root_focused -and (!$RequireJumpRootOwner -or $candidate.root_owner_allowed)
    } $CaptureTimeoutSeconds "$Description owned ROOT baseline"
    if ($state.query_focused) { return $state }

    if ($RequireJumpRootOwner) {
        if (!$state.jump_mode_enabled) { throw "$Description requires Jump Mode to focus the query field without Tab traversal" }
        $queryBefore = [string]$state.query
        $activationCountBefore = [int]$state.activation_count
        Invoke-NativeKeyBatch "${Description}-shift-space-query-focus" ([uint16[]]@(0x10, 0x20)) ([string[]]@('Shift', 'Space'))
        $active = Wait-JumpModeObservation {
            param($candidate)
            $candidate.root_focused -and $candidate.root_owner_allowed -and $candidate.jump_mode_enabled -and
                $candidate.active_session -and $candidate.query -ceq $queryBefore -and
                $candidate.activation_count -eq $activationCountBefore
        } $CaptureTimeoutSeconds "$Description Jump Mode active map before query focus"
        $queryTarget = Get-Phase1QueryTarget $active
        $queryTargetId = [string]$queryTarget.target_id
        $queryHint = [string]$queryTarget.hint
        Send-ObservedHint "${Description}-observed-query-hint" $queryHint
        $focused = Wait-JumpModeObservation {
            param($candidate)
            $candidate.root_focused -and $candidate.root_owner_allowed -and !$candidate.active_session -and
                $candidate.outcome.kind -eq 'completed' -and $candidate.outcome.target_id -ceq $queryTargetId -and
                $candidate.focused_destination -ceq $queryTargetId -and $candidate.query_focused -and
                $candidate.query -ceq $queryBefore -and $candidate.activation_count -eq $activationCountBefore
        } $CaptureTimeoutSeconds "$Description observed query target focus handoff"
        Write-Event 'phase1_query_focus_handoff' @{
            target_id = $queryTargetId
            hint = $queryHint
            query = $focused.query
            query_focused = $focused.query_focused
            activation_count = $focused.activation_count
            query_before = $queryBefore
        }
        return $focused
    }

    for ($tabIndex = 1; $tabIndex -le 12; $tabIndex++) {
        if ($state.query_focused) { return $state }
        $previousFrame = [int64]$state.logic_frame_id
        $previousTabCount = [int64]$state.tab_processed_count
        Invoke-NativeKeyBatch "${Description}-tab-query-focus-$tabIndex" ([uint16[]]@(0x09)) ([string[]]@('Tab'))
        $state = Wait-JumpModeObservation {
            param($candidate)
            $candidate.root_focused -and (!$RequireJumpRootOwner -or $candidate.root_owner_allowed) -and
                [int64]$candidate.tab_processed_count -gt $previousTabCount -and
                $null -ne $candidate.last_tab_processed_logic_frame_id -and
                [int64]$candidate.last_tab_processed_logic_frame_id -gt $previousFrame
        } 5 "$Description ROOT frame after Tab $tabIndex"
    }
    if (!$state.query_focused) { throw "$Description could not reach the actual query field with bounded native Tab traversal" }
    return $state
}

function Save-Phase1Capture([int]$CatalogNumber, [string]$Name, [scriptblock]$Predicate, [string]$Description) {
    $captureIndex = $phase1NextCaptureIndex
    $captureCount = $phase1CaptureCount
    if ($captureIndex -gt $captureCount) { throw 'Phase 1 requested more captures than the QA request allows' }
    $readyState = Wait-JumpModeObservation {
        param($state)
        $state.capture_armed -and [int]$state.capture_index -eq $captureIndex -and
            [int]$state.capture_count -eq $captureCount -and (& $Predicate $state)
    } $CaptureTimeoutSeconds "catalog $CatalogNumber ($Description) retained state and capture rearm"
    Write-Event 'phase1_capture_requesting' @{
        catalog_number = $CatalogNumber
        name = $Name
        capture_index = $captureIndex
        logic_frame_id = $readyState.logic_frame_id
        ui_pass_id = $readyState.ui_pass_id
        query = $readyState.query
        activation_count = $readyState.activation_count
        prefix = $readyState.prefix
    }
    if ($captureCount -eq 1) {
        $receiptFileName = 'receipt.json'
        $sourcePngFileName = 'framebuffer.png'
    } else {
        $receiptFileName = 'receipt-{0:D3}.json' -f $captureIndex
        $sourcePngFileName = 'framebuffer-{0:D3}.png' -f $captureIndex
    }
    $receiptPath = Join-Path $captureRoot $receiptFileName
    $sourcePngPath = Join-Path $captureRoot $sourcePngFileName
    if ((Test-Path -LiteralPath $receiptPath) -or (Test-Path -LiteralPath $sourcePngPath)) {
        throw "QA capture output for index $captureIndex already exists"
    }
    Invoke-NativeKeyBatch "catalog-$CatalogNumber-f11-capture" ([uint16[]]@(0x7A)) ([string[]]@('F11'))
    $deadline = Get-RunDeadline $CaptureTimeoutSeconds
    $receipt = $null
    while ([DateTime]::UtcNow -lt $deadline) {
        $process.Refresh()
        if ($process.HasExited) {
            $script:processExitCode = $process.ExitCode
            throw "isolated app exited while saving catalog capture $CatalogNumber (exit $script:processExitCode)"
        }
        if (Test-Path -LiteralPath $receiptPath -PathType Leaf) {
            try { $candidate = Get-Content -Raw -LiteralPath $receiptPath | ConvertFrom-Json } catch { $candidate = $null }
            if ($candidate.status -eq 'failed') { throw "QA capture $captureIndex failed: $($candidate.failure)" }
            if ($candidate.status -eq 'saved' -and (Test-Path -LiteralPath $sourcePngPath -PathType Leaf)) {
                $receipt = $candidate
                break
            }
        }
        Start-Sleep -Milliseconds 100
    }
    if (!$receipt) { throw "timed out after $CaptureTimeoutSeconds seconds waiting for catalog capture $CatalogNumber receipt" }
    if ([int]$receipt.capture_index -ne $captureIndex -or [int]$receipt.capture_count -ne $captureCount) { throw 'capture receipt index/count does not match the armed request' }
    if ($receipt.trigger -ne 'f11' -or $receipt.target -ne 'ROOT') { throw 'capture receipt does not identify the required ROOT/F11 boundary' }
    if ([int]$receipt.process.pid -ne $processId) { throw 'capture receipt PID does not match the retained app process' }
    if ([string]$receipt.executable.sha256 -ne $executableHash) { throw 'capture receipt executable hash does not match the built binary' }
    if ([string]$receipt.fixture_manifest_sha256 -ne $fixtureHash) { throw 'capture receipt fixture hash does not match the verified profile' }
    if ([string]$receipt.build_receipt_identity -cne $buildReceiptIdentity) { throw 'capture receipt build identity does not match the launched binary' }
    $captured = $receipt.captured
    $capturedObservation = $captured.jump_mode_observation
    if (!$captured -or !$captured.eligible -or !$capturedObservation) { throw 'capture receipt lacks eligible retained ROOT state and its Jump Mode observation' }
    if ([string]$captured.viewport_id -cne 'ROOT') { throw 'capture receipt captured a viewport other than ROOT' }
    if ([int]$capturedObservation.process_pid -ne $processId -or [string]$capturedObservation.fixture_manifest_sha256 -cne $fixtureHash) { throw 'captured state PID/fixture identity does not match the launched isolated app' }
    if ([int]$capturedObservation.capture_index -ne $captureIndex -or [int]$capturedObservation.capture_count -ne $captureCount) { throw 'captured state index/count does not match the matching receipt' }
    if ([int64]$captured.logic_frame_id -ne [int64]$capturedObservation.logic_frame_id -or
        [int64]$captured.ui_pass_id -ne [int64]$capturedObservation.ui_pass_id) { throw 'receipt and Jump Mode snapshot identify different retained frames or UI passes' }
    if ([string]$receipt.capture_semantics -cne 'ROOT framebuffer pixels; CPU UI completion before GPU presentation') { throw 'capture receipt semantics do not identify the native ROOT framebuffer boundary' }
    if (!(& $Predicate $capturedObservation)) { throw "catalog $CatalogNumber receipt captured state does not satisfy $Description" }
    $png = Get-PngDimensions $sourcePngPath
    $pngHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $sourcePngPath).Hash.ToLowerInvariant()
    if ([string]$receipt.saved.png_sha256 -cne $pngHash) { throw 'saved PNG hash does not match its native ROOT capture receipt' }
    $screenshotFileName = '{0:D2}_{1}.png' -f $CatalogNumber, $Name
    $screenshotRelativePath = "screenshots/$screenshotFileName"
    $screenshotDestination = Join-Path $screenshotsRoot $screenshotFileName
    Copy-Item -LiteralPath $sourcePngPath -Destination $screenshotDestination
    $copyHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $screenshotDestination).Hash.ToLowerInvariant()
    if ($copyHash -cne $pngHash) { throw 'catalog screenshot copy changed the captured PNG bytes' }
    $record = [ordered]@{
        catalog_number = $CatalogNumber
        name = $Name
        status = 'PASS'
        screenshot = $screenshotRelativePath
        screenshot_sha256 = $pngHash
        capture_index = $captureIndex
        capture_count = $captureCount
        receipt = "capture/$receiptFileName"
        process_pid = $processId
        executable_sha256 = $executableHash
        fixture_manifest_sha256 = $fixtureHash
        logic_frame_id = $captured.logic_frame_id
        ui_pass_id = $captured.ui_pass_id
        query = $capturedObservation.query
        result_generation = $capturedObservation.result_generation
        result_count = $capturedObservation.result_count
        selected_index = $capturedObservation.selected_index
        selected_target_id = $capturedObservation.selected_target_id
        activation_count = $capturedObservation.activation_count
        active_session = $capturedObservation.active_session
        prefix = $capturedObservation.prefix
        pixels_per_point = $captured.pixels_per_point
        native_pixels_per_point = $captured.native_pixels_per_point
        zoom_factor = $captured.zoom_factor
    }
    $script:phase1Captures.Add($record)
    $script:phase1NextCaptureIndex = $captureIndex + 1
    $script:captureReceipt = $receipt
    $script:screenshotPath = $screenshotRelativePath
    $script:screenshotWidth = $png.Width
    $script:screenshotHeight = $png.Height
    Update-Scenarios
    Write-Event 'phase1_capture_saved' $record
    if ($captureIndex -lt $captureCount) {
        $nextIndex = $captureIndex + 1
        $null = Wait-JumpModeObservation {
            param($state)
            $state.capture_armed -and [int]$state.capture_index -eq $nextIndex -and
                [int]$state.capture_count -eq $captureCount
        } $CaptureTimeoutSeconds "capture $captureIndex saved and capture $nextIndex rearmed"
    }
    return $record
}

function Invoke-Phase1EnabledWorkflow {
    $script:phase1BaseActivationCount = if ($Screen -eq 'Grid') { 0 } else { 1 }
    Write-Event 'phase1_enabled_native_sequence_started' @{ expected_capture_count = $phase1CaptureCount; screen = $Screen; jump_mode_enabled = $true }
    if ($Screen -ne 'Grid') {
    $baseline = Ensure-Phase1QueryFocus 'phase1-dashboard'
    $baseline = Wait-JumpModeObservation {
        param($state)
        $dashboardTargets = @($state.targets | Where-Object { $_.surface -eq 'dashboard' -and $_.eligible })
        $state.root_focused -and $state.root_owner_allowed -and $state.jump_mode_enabled -and
            !$state.active_session -and $state.query -ceq '' -and $state.activation_count -eq 0 -and
            $dashboardTargets.Count -gt 0
    } $CaptureTimeoutSeconds 'isolated populated dashboard before native input'
    $null = Save-Phase1Capture 1 'dashboard_normal' {
        param($state)
        $dashboardTargets = @($state.targets | Where-Object { $_.surface -eq 'dashboard' -and $_.eligible })
        $state.root_focused -and $state.root_owner_allowed -and $state.jump_mode_enabled -and
            !$state.active_session -and $state.query -ceq '' -and $state.activation_count -eq 0 -and
            $dashboardTargets.Count -gt 0
    } 'populated safe dashboard with Jump Mode enabled and inactive'

    Invoke-NativeKeyBatch 'dashboard-shift-space-leader' ([uint16[]]@(0x10, 0x20)) ([string[]]@('Shift', 'Space'))
    $dashboardActive = Wait-JumpModeObservation {
        param($state)
        $dashboardTargets = @($state.targets | Where-Object { $_.surface -eq 'dashboard' -and $_.eligible -and $_.hint })
        $state.root_focused -and $state.root_owner_allowed -and $state.jump_mode_enabled -and
            $state.active_session -and $state.prefix -ceq '' -and $state.query -ceq '' -and
            $state.activation_count -eq 0 -and $dashboardTargets.Count -gt 0
    } $CaptureTimeoutSeconds 'active dashboard target map with unchanged query and zero activations'
    $dashboardTarget = Get-Phase1Target $dashboardActive {
        param($target)
        $target.surface -eq 'dashboard' -and $target.label -cmatch '^Jump QA Note [0-9]{3}$' -and
            $target.kind -eq 'button' -and $target.hint.Length -ge 2
    } 'a multi-character fixture note card configured for safe FillQuery'
    $dashboardTargetId = [string]$dashboardTarget.target_id
    $dashboardHint = [string]$dashboardTarget.hint
    Write-Event 'phase1_dashboard_hint_observed' @{
        target_id = $dashboardTargetId
        label = $dashboardTarget.label
        hint = $dashboardHint
        surface = $dashboardTarget.surface
        activation_count = $dashboardActive.activation_count
    }
    $dashboardActivePredicate = {
        param($state)
        $dashboardTargets = @($state.targets | Where-Object { $_.surface -eq 'dashboard' -and $_.eligible -and $_.hint })
        $state.root_focused -and $state.root_owner_allowed -and $state.jump_mode_enabled -and
            $state.active_session -and $state.prefix -ceq '' -and $state.query -ceq '' -and
            $state.activation_count -eq 0 -and $dashboardTargets.Count -gt 0
    }.GetNewClosure()
    $null = Save-Phase1Capture 2 'dashboard_hints_active' $dashboardActivePredicate 'active dashboard hint map'

    Send-ObservedHint 'dashboard-prefix' $dashboardHint.Substring(0, 1)
    $dashboardPartial = Wait-JumpModeObservation {
        param($state)
        $state.root_focused -and $state.root_owner_allowed -and $state.jump_mode_enabled -and
            $state.active_session -and $state.prefix -ceq $dashboardHint.Substring(0, 1) -and
            $state.prefix_target_ids -contains $dashboardTargetId -and
            $state.prefix -cne $dashboardHint -and $state.query -ceq '' -and $state.activation_count -eq 0
    } $CaptureTimeoutSeconds 'multi-character dashboard hint prefix'
    Write-Event 'phase1_dashboard_partial_prefix_observed' @{
        target_id = $dashboardTargetId
        hint = $dashboardHint
        prefix = $dashboardPartial.prefix
        matching_target_ids = @($dashboardPartial.prefix_target_ids)
        query = $dashboardPartial.query
        activation_count = $dashboardPartial.activation_count
    }
    $dashboardPartialPredicate = {
        param($state)
        $state.root_focused -and $state.root_owner_allowed -and $state.jump_mode_enabled -and
            $state.active_session -and $state.prefix -ceq $dashboardHint.Substring(0, 1) -and
            $state.prefix_target_ids -contains $dashboardTargetId -and
            $state.prefix -cne $dashboardHint -and $state.query -ceq '' -and $state.activation_count -eq 0
    }.GetNewClosure()
    $null = Save-Phase1Capture 3 'dashboard_multichar_prefix' $dashboardPartialPredicate 'partial multi-character dashboard hint'

    if ($dashboardHint.Length -gt 1) { Send-ObservedHint 'dashboard-hint-remainder' $dashboardHint.Substring(1) }
    $dashboardSelected = Wait-JumpModeObservation {
        param($state)
        $focusedTarget = @($state.targets | Where-Object { $_.target_id -ceq $dashboardTargetId -and $_.focused })
        $state.root_focused -and $state.root_owner_allowed -and !$state.active_session -and
            $state.outcome.kind -eq 'completed' -and $state.outcome.target_id -ceq $dashboardTargetId -and
            $state.focused_destination -ceq $dashboardTargetId -and $focusedTarget.Count -eq 1 -and
            $state.query -ceq '' -and $state.activation_count -eq 0
    } $CaptureTimeoutSeconds 'dashboard hint completion focused the exact card without activating it'
    Write-Event 'phase1_dashboard_target_focused' @{
        target_id = $dashboardTargetId
        label = $dashboardTarget.label
        outcome = $dashboardSelected.outcome
        focused_destination = $dashboardSelected.focused_destination
        query = $dashboardSelected.query
        activation_count = $dashboardSelected.activation_count
    }
    $dashboardSelectedPredicate = {
        param($state)
        $focusedTarget = @($state.targets | Where-Object { $_.target_id -ceq $dashboardTargetId -and $_.focused })
        $state.root_focused -and $state.root_owner_allowed -and !$state.active_session -and
            $state.outcome.kind -eq 'completed' -and $state.outcome.target_id -ceq $dashboardTargetId -and
            $state.focused_destination -ceq $dashboardTargetId -and $focusedTarget.Count -eq 1 -and
            $state.query -ceq '' -and $state.activation_count -eq 0
    }.GetNewClosure()
    $null = Save-Phase1Capture 4 'dashboard_card_selected' $dashboardSelectedPredicate 'exact dashboard card focused before a fresh Enter'

    Invoke-NativeKeyBatch 'dashboard-card-fresh-enter' ([uint16[]]@(0x0D)) ([string[]]@('Enter'))
    $fillQueryActionId = "query:$($dashboardTarget.label)"
    $filledQuery = Wait-JumpModeObservation {
        param($state)
        $state.root_focused -and $state.root_owner_allowed -and !$state.active_session -and
            $state.query -ceq $dashboardTarget.label -and $state.activation_count -eq 1 -and
            $state.last_action.action_id -ceq $fillQueryActionId -and
            $state.last_action.query_override -ceq $dashboardTarget.label -and
            $state.last_action.source -ceq 'dashboard'
    } $CaptureTimeoutSeconds 'one Dashboard-sourced safe FillQuery activation'
    Write-Event 'phase1_dashboard_fill_query_observed' @{
        target_id = $dashboardTargetId
        action_id = $filledQuery.last_action.action_id
        args = $filledQuery.last_action.args
        query_override = $filledQuery.last_action.query_override
        source = $filledQuery.last_action.source
        query = $filledQuery.query
        activation_count = $filledQuery.activation_count
    }
    $filledQueryPredicate = {
        param($state)
        $state.root_focused -and $state.root_owner_allowed -and !$state.active_session -and
            $state.query -ceq $dashboardTarget.label -and $state.activation_count -eq 1 -and
            $state.last_action.action_id -ceq $fillQueryActionId -and
            $state.last_action.query_override -ceq $dashboardTarget.label -and
            $state.last_action.source -ceq 'dashboard'
    }.GetNewClosure()
    $null = Save-Phase1Capture 5 'dashboard_fill_query_once' $filledQueryPredicate 'one fresh Enter activated the safe dashboard FillQuery action'
    }
    if ($Screen -eq 'Dashboard') {
        if ($phase1Captures.Count -ne 5) { throw "Dashboard filter saved $($phase1Captures.Count) of 5 catalog captures" }
        $script:scenarioStatus = 'PASS'
        $script:scenarioReason = 'Dashboard filter captured catalog 01–05 with receipt-bound ROOT/F11 evidence and verified safe FillQuery semantics.'
        $script:runStatus = 'PASS'
        $script:phase = 'phase1_dashboard_complete'
        Update-Scenarios
        Write-Event 'phase1_dashboard_assertions_passed' @{ capture_count = $phase1Captures.Count; activation_count = 1 }
        Write-Summary 'PASS' 'Dashboard-only filter completed its five receipt-bound native capture states.'
        return
    }

    if ($Screen -eq 'Grid') {
        $null = Ensure-Phase1QueryFocus 'phase1-grid'
        Send-NativeText 'grid-filter-note-list' 'note list'
        $noteList = Wait-JumpModeObservation {
            param($state)
            $resultTargets = @(Get-Phase1ResultTargets $state)
            $state.root_focused -and $state.root_owner_allowed -and $state.jump_mode_enabled -and
                !$state.active_session -and $state.query -ceq 'note list' -and
                $state.result_count -eq 32 -and $state.grid_layout -and $state.query_focused -and
                $resultTargets.Count -gt 0 -and $state.activation_count -eq 0
        } $CaptureTimeoutSeconds 'Grid filter typed note-list query with 32 resolved results'
    } else {
    Invoke-NativeKeyBatch 'query-select-all-before-note-list' ([uint16[]]@(0x11, 0x41)) ([string[]]@('Ctrl', 'A'))
    Send-NativeText 'query-note-list' 'note list'
    $noteList = Wait-JumpModeObservation {
        param($state)
        $resultTargets = @(Get-Phase1ResultTargets $state)
        $state.root_focused -and $state.root_owner_allowed -and $state.jump_mode_enabled -and
            !$state.active_session -and $state.query -ceq 'note list' -and
            $state.result_count -eq 32 -and $state.grid_layout -and $state.query_focused -and
            $resultTargets.Count -gt 0 -and $state.activation_count -eq $phase1BaseActivationCount
    } $CaptureTimeoutSeconds 'typed note-list query with 32 resolved grid results'
    }
    Write-Event 'phase1_note_grid_ready' @{
        query = $noteList.query
        result_count = $noteList.result_count
        result_generation = $noteList.result_generation
        grid_layout = $noteList.grid_layout
        visible_result_targets = @(Get-Phase1ResultTargets $noteList).Count
        activation_count = $noteList.activation_count
    }
    $noteListPredicate = {
        param($state)
        $state.root_focused -and $state.root_owner_allowed -and $state.jump_mode_enabled -and
            !$state.active_session -and $state.query -ceq 'note list' -and
            $state.result_count -eq 32 -and $state.grid_layout -and $state.query_focused -and
            $state.activation_count -eq $phase1BaseActivationCount
    }
    $null = Save-Phase1Capture 6 'note_grid_normal' $noteListPredicate '32 note results resolved in the configured grid'

    Invoke-NativeKeyBatch 'note-grid-shift-space-leader' ([uint16[]]@(0x10, 0x20)) ([string[]]@('Shift', 'Space'))
    $noteGridActive = Wait-JumpModeObservation {
        param($state)
        $resultTargets = @(Get-Phase1ResultTargets $state)
        $state.root_focused -and $state.root_owner_allowed -and $state.jump_mode_enabled -and
            $state.active_session -and $state.prefix -ceq '' -and $state.query -ceq 'note list' -and
            $state.result_count -eq 32 -and $state.grid_layout -and $resultTargets.Count -gt 26 -and
            $state.activation_count -eq $phase1BaseActivationCount
    } $CaptureTimeoutSeconds 'dense note grid active hint map'
    $allHintTargets = @($noteGridActive.targets | Where-Object { $_.eligible -and $_.hint })
    Assert-Phase1PrefixFree $allHintTargets 'active note grid target map'
    $noteTarget = Get-Phase1Target $noteGridActive {
        param($target)
        $target.surface -eq 'launcher_results' -and $null -ne $target.root_action -and
            $target.root_action.action -ceq 'note:open:jump-qa-note-001'
    } 'the exact typed root result for duplicate-title note slug jump-qa-note-001'
    $noteTargetId = [string]$noteTarget.target_id
    $noteTargetHint = [string]$noteTarget.hint
    $noteTargetIndex = [int]$noteTarget.root_result_index
    if ($null -eq $noteTarget.root_result_index -or [int64]$noteGridActive.result_generation -le 0) { throw 'typed note target is not bound to the current root result generation' }
    Write-Event 'phase1_note_grid_hint_observed' @{
        target_id = $noteTargetId
        root_result_index = $noteTargetIndex
        result_generation = $noteGridActive.result_generation
        action = $noteTarget.root_action
        hint = $noteTargetHint
        active_target_count = $allHintTargets.Count
        prefix_free = $true
    }
    $noteGridActivePredicate = {
        param($state)
        $resultTargets = @(Get-Phase1ResultTargets $state)
        $state.root_focused -and $state.root_owner_allowed -and $state.jump_mode_enabled -and
            $state.active_session -and $state.prefix -ceq '' -and $state.query -ceq 'note list' -and
            $state.result_count -eq 32 -and $state.grid_layout -and $resultTargets.Count -gt 26 -and
            $state.activation_count -eq $phase1BaseActivationCount
    }
    $null = Save-Phase1Capture 7 'note_grid_hints_active' $noteGridActivePredicate '32-result grid with more than 26 active visible targets'

    if ($noteTargetHint.Length -gt 1) {
        Send-ObservedHint 'note-result-prefix' $noteTargetHint.Substring(0, 1)
        $notePartial = Wait-JumpModeObservation {
            param($state)
            $state.active_session -and $state.prefix -ceq $noteTargetHint.Substring(0, 1) -and
                $state.prefix -cne $noteTargetHint -and $state.prefix_target_ids -contains $noteTargetId -and
                $state.query -ceq 'note list' -and $state.activation_count -eq $phase1BaseActivationCount
        } $CaptureTimeoutSeconds 'multi-character note-result hint prefix'
        Write-Event 'phase1_note_grid_partial_prefix_observed' @{
            target_id = $noteTargetId
            hint = $noteTargetHint
            prefix = $notePartial.prefix
            matching_target_ids = @($notePartial.prefix_target_ids)
            activation_count = $notePartial.activation_count
        }
        Send-ObservedHint 'note-result-hint-remainder' $noteTargetHint.Substring(1)
    } else {
        Send-ObservedHint 'note-result-single-character-hint' $noteTargetHint
        $notePartial = Wait-JumpModeObservation {
            param($state)
            !$state.active_session -and $state.outcome.kind -eq 'completed' -and
                $state.outcome.target_id -ceq $noteTargetId -and
                $state.selected_target_id -ceq $noteTargetId -and $state.selected_index -eq $noteTargetIndex -and
                $state.selected_action.action -ceq 'note:open:jump-qa-note-001' -and
                $state.query -ceq 'note list' -and $state.activation_count -eq $phase1BaseActivationCount
        } $CaptureTimeoutSeconds 'single-character hint selected the exact duplicate-title note slug'
        Write-Event 'phase1_note_grid_single_character_target_selected' @{
            target_id = $noteTargetId
            hint = $noteTargetHint
            selected_index = $notePartial.selected_index
            selected_action = $notePartial.selected_action
            result_generation = $notePartial.result_generation
            activation_count = $notePartial.activation_count
        }
    }
    $noteSelected = Wait-JumpModeObservation {
        param($state)
        $typedTarget = @($state.targets | Where-Object {
            $_.target_id -ceq $noteTargetId -and $_.root_result_index -eq $noteTargetIndex -and
                $_.root_action.action -ceq 'note:open:jump-qa-note-001'
        })
        !$state.active_session -and $state.outcome.kind -eq 'completed' -and
            $state.outcome.target_id -ceq $noteTargetId -and
            $state.query_focused -and $state.focused_destination -ceq $state.query_target_id -and $typedTarget.Count -eq 1 -and
            $state.query -ceq 'note list' -and $state.result_count -eq 32 -and
            $state.selected_index -eq $noteTargetIndex -and $state.selected_target_id -ceq $noteTargetId -and
            $state.selected_action.action -ceq 'note:open:jump-qa-note-001' -and
            $state.selected_action.args -ceq $noteTarget.root_action.args -and $state.activation_count -eq $phase1BaseActivationCount
    } $CaptureTimeoutSeconds 'selected note result with typed action, current index, and exact focus destination'
    $focusedTypedTarget = @($noteSelected.targets | Where-Object { $_.target_id -ceq $noteTargetId })[0]
    if ([int64]$noteSelected.result_generation -ne [int64]$noteGridActive.result_generation) { throw 'root result generation changed between note hint assignment and selection' }
    if ([int]$noteSelected.selected_index -ne $noteTargetIndex -or $noteSelected.selected_target_id -cne $noteTargetId) { throw 'actual selected result identity does not match the focused note target' }
    if ($noteSelected.selected_action.action -cne 'note:open:jump-qa-note-001' -or $noteSelected.selected_action.args -cne $noteTarget.root_action.args) { throw 'selected typed action payload does not match the focused note target' }
    Write-Event 'phase1_note_result_selected' @{
        target_id = $noteTargetId
        result_index = $focusedTypedTarget.root_result_index
        result_generation = $noteSelected.result_generation
        typed_action = $focusedTypedTarget.root_action
        selected_index = $noteSelected.selected_index
        selected_action = $noteSelected.selected_action
        focused_destination = $noteSelected.focused_destination
        activation_count = $noteSelected.activation_count
    }
    $noteSelectedPredicate = {
        param($state)
        $typedTarget = @($state.targets | Where-Object {
            $_.target_id -ceq $noteTargetId -and $_.root_result_index -eq $noteTargetIndex -and
                $_.root_action.action -ceq 'note:open:jump-qa-note-001'
        })
        !$state.active_session -and $state.outcome.kind -eq 'completed' -and
            $state.outcome.target_id -ceq $noteTargetId -and
            $state.query_focused -and $state.focused_destination -ceq $state.query_target_id -and $typedTarget.Count -eq 1 -and
            $state.query -ceq 'note list' -and $state.result_count -eq 32 -and
            $state.selected_index -eq $noteTargetIndex -and $state.selected_target_id -ceq $noteTargetId -and
            $state.selected_action.action -ceq 'note:open:jump-qa-note-001' -and
            $state.selected_action.args -ceq $noteTarget.root_action.args -and $state.activation_count -eq $phase1BaseActivationCount
    }.GetNewClosure()
    $null = Save-Phase1Capture 8 'note_result_selected' $noteSelectedPredicate 'exact note result selected without activating it'

    $staleTargetGeneration = [int64]$noteSelected.result_generation
    Invoke-NativeKeyBatch 'stale-target-query-select-all' ([uint16[]]@(0x11, 0x41)) ([string[]]@('Ctrl', 'A'))
    Send-NativeText 'stale-target-no-results-query' $staleInvalidationQuery
    $staleTargetInvalidated = Wait-JumpModeObservation {
        param($state)
        $staleActionTargets = @((Get-Phase1ResultTargets $state) | Where-Object { $_.root_action.action -ceq 'note:open:jump-qa-note-001' })
        !$state.active_session -and $state.root_focused -and $state.root_owner_allowed -and
            $state.query_focused -and $state.query -ceq $staleInvalidationQuery -and $state.result_count -eq 0 -and
            [int64]$state.result_generation -gt $staleTargetGeneration -and
            $state.selected_target_id -cne $noteTargetId -and $staleActionTargets.Count -eq 0 -and
            $state.activation_count -eq $phase1BaseActivationCount -and @($state.opened_note_slugs).Count -eq 0
    } $CaptureTimeoutSeconds 'safe no-results query invalidated the selected root target and advanced result generation'
    Write-Event 'phase1_stale_target_invalidated' @{
        stale_target_id = $noteTargetId
        stale_result_index = $noteTargetIndex
        old_result_generation = $staleTargetGeneration
        new_result_generation = $staleTargetInvalidated.result_generation
        invalidation_query = $staleInvalidationQuery
        result_count = $staleTargetInvalidated.result_count
        selected_target_id = $staleTargetInvalidated.selected_target_id
        activation_count = $staleTargetInvalidated.activation_count
        opened_note_slugs = @($staleTargetInvalidated.opened_note_slugs)
    }
    $staleEnterProcessedCount = [int64]$staleTargetInvalidated.enter_processed_count
    Invoke-NativeKeyBatch 'stale-note-target-fresh-enter' ([uint16[]]@(0x0D)) ([string[]]@('Enter'))
    $staleEnterState = Wait-JumpModeObservation {
        param($state)
        !$state.active_session -and [int64]$state.logic_frame_id -gt [int64]$staleTargetInvalidated.logic_frame_id -and
            [int64]$state.enter_processed_count -gt $staleEnterProcessedCount -and
            $null -ne $state.last_enter_processed_logic_frame_id -and
            [int64]$state.last_enter_processed_logic_frame_id -gt [int64]$staleTargetInvalidated.logic_frame_id -and
            $state.query_focused -and $state.query -ceq $staleInvalidationQuery -and $state.result_count -eq 0 -and
            [int64]$state.result_generation -ge [int64]$staleTargetInvalidated.result_generation -and
            $state.activation_count -eq $phase1BaseActivationCount -and @($state.opened_note_slugs).Count -eq 0 -and
            $state.last_action.action_id -cne 'note:open:jump-qa-note-001'
    } $CaptureTimeoutSeconds 'processed Enter after target invalidation did not execute the stale note or another action'
    Write-Event 'phase1_stale_target_enter_blocked' @{
        stale_target_id = $noteTargetId
        stale_result_generation = $staleTargetGeneration
        current_result_generation = $staleEnterState.result_generation
        invalidation_query = $staleInvalidationQuery
        activation_count = $staleEnterState.activation_count
        enter_processed_count = $staleEnterState.enter_processed_count
        last_enter_processed_logic_frame_id = $staleEnterState.last_enter_processed_logic_frame_id
        selected_target_id = $staleEnterState.selected_target_id
        last_action = $staleEnterState.last_action
    }
    Invoke-NativeKeyBatch 'stale-target-restore-query-select-all' ([uint16[]]@(0x11, 0x41)) ([string[]]@('Ctrl', 'A'))
    Send-NativeText 'stale-target-restore-note-list' 'note list'
    $restoredNotes = Wait-JumpModeObservation {
        param($state)
        !$state.active_session -and $state.query_focused -and $state.query -ceq 'note list' -and $state.result_count -eq 32 -and
            [int64]$state.result_generation -gt [int64]$staleTargetInvalidated.result_generation -and
            $state.activation_count -eq $phase1BaseActivationCount -and @($state.opened_note_slugs).Count -eq 0
    } $CaptureTimeoutSeconds 'note-list query restored 32 production results in the next root generation'
    Invoke-NativeKeyBatch 'restored-note-result-shift-space-leader' ([uint16[]]@(0x10, 0x20)) ([string[]]@('Shift', 'Space'))
    $restoredActive = Wait-JumpModeObservation {
        param($state)
        $resultTargets = @(Get-Phase1ResultTargets $state)
        $state.active_session -and $state.prefix -ceq '' -and $state.query -ceq 'note list' -and
            $state.result_count -eq 32 -and $state.grid_layout -and $resultTargets.Count -gt 26 -and
            [int64]$state.result_generation -eq [int64]$restoredNotes.result_generation -and
            $state.activation_count -eq $phase1BaseActivationCount
    } $CaptureTimeoutSeconds 'fresh hint map after restoring isolated note fixture'
    $freshNoteTarget = Get-Phase1Target $restoredActive {
        param($target)
        $target.surface -eq 'launcher_results' -and $null -ne $target.root_action -and
            $target.root_action.action -ceq 'note:open:jump-qa-note-001'
    } 'the restored typed root result for note slug jump-qa-note-001'
    Send-ObservedHint 'restored-note-result-hint' ([string]$freshNoteTarget.hint)
    $noteSelected = Wait-JumpModeObservation {
        param($state)
        $typedTarget = @($state.targets | Where-Object {
            $_.target_id -ceq $freshNoteTarget.target_id -and $_.root_result_index -eq $freshNoteTarget.root_result_index -and
                $_.root_action.action -ceq 'note:open:jump-qa-note-001'
        })
        !$state.active_session -and $state.outcome.kind -eq 'completed' -and
            $state.outcome.target_id -ceq $freshNoteTarget.target_id -and
            $state.query_focused -and $state.focused_destination -ceq $state.query_target_id -and
            $state.selected_target_id -ceq $freshNoteTarget.target_id -and
            $state.selected_index -eq $freshNoteTarget.root_result_index -and $typedTarget.Count -eq 1 -and
            [int64]$state.result_generation -eq [int64]$restoredActive.result_generation -and
            $state.activation_count -eq $phase1BaseActivationCount
    } $CaptureTimeoutSeconds 'restored note hint selected the same typed note in its new result generation'
    $noteTarget = $freshNoteTarget
    $noteTargetId = [string]$noteTarget.target_id
    $noteTargetIndex = [int]$noteTarget.root_result_index
    Write-Event 'phase1_restored_note_target_reselected' @{
        target_id = $noteTargetId
        root_result_index = $noteTargetIndex
        result_generation = $noteSelected.result_generation
        action = $noteSelected.selected_action
        activation_count = $noteSelected.activation_count
    }

    Invoke-NativeKeyBatch 'note-result-fresh-enter' ([uint16[]]@(0x0D)) ([string[]]@('Enter'))
    $noteOpened = Wait-JumpModeObservation {
        param($state)
        $opened = @($state.opened_note_slugs)
        $state.activation_count -eq ($phase1BaseActivationCount + 1) -and $state.last_action.action_id -ceq 'note:open:jump-qa-note-001' -and
            $state.last_action.source -ceq 'enter' -and $state.last_action.args -ceq $noteTarget.root_action.args -and
            $state.query -ceq 'note list' -and $opened.Count -eq 1 -and $opened[0] -ceq 'jump-qa-note-001'
    } $CaptureTimeoutSeconds 'fresh Enter opened exactly the selected isolated note slug once'
    Write-Event 'phase1_note_opened' @{
        target_id = $noteTargetId
        action_id = $noteOpened.last_action.action_id
        args = $noteOpened.last_action.args
        source = $noteOpened.last_action.source
        opened_note_slugs = @($noteOpened.opened_note_slugs)
        activation_count = $noteOpened.activation_count
    }
    $noteOpenedPredicate = {
        param($state)
        $opened = @($state.opened_note_slugs)
        $state.activation_count -eq ($phase1BaseActivationCount + 1) -and $state.last_action.action_id -ceq 'note:open:jump-qa-note-001' -and
            $state.last_action.source -ceq 'enter' -and $state.last_action.args -ceq $noteTarget.root_action.args -and
            $state.query -ceq 'note list' -and $opened.Count -eq 1 -and $opened[0] -ceq 'jump-qa-note-001'
    }.GetNewClosure()
    $null = Save-Phase1Capture 9 'note_opened_by_fresh_enter' $noteOpenedPredicate 'fresh Enter opened exactly the typed note target once'

    $noteOwnerPredicate = {
        param($state)
        $opened = @($state.opened_note_slugs)
        $rootTargets = @($state.targets | Where-Object { $_.surface -in @('dashboard', 'launcher_results') })
        !$state.active_session -and $opened.Count -eq 1 -and $opened[0] -ceq 'jump-qa-note-001' -and
            $rootTargets.Count -eq 0 -and $state.query -ceq 'note list' -and $state.activation_count -eq ($phase1BaseActivationCount + 1)
    }
    $null = Save-Phase1Capture 14 'note_panel_blocks_root_targets' $noteOwnerPredicate 'open note panel owns interaction and blocks all root targets'

    Invoke-NativeKeyBatch 'close-unmodified-note-panel-escape' ([uint16[]]@(0x1B)) ([string[]]@('Escape'))
    $closedNote = Wait-JumpModeObservation {
        param($state)
        $state.root_focused -and $state.root_owner_allowed -and $state.opened_note_slugs.Count -eq 0 -and
            $state.query -ceq 'note list' -and $state.activation_count -eq ($phase1BaseActivationCount + 1) -and !$state.active_session
    } $CaptureTimeoutSeconds 'safe Escape close of the unmodified note panel'
    Write-Event 'phase1_unmodified_note_closed' @{ opened_note_slugs = @($closedNote.opened_note_slugs); query = $closedNote.query; activation_count = $closedNote.activation_count }

    $queryFocusBaseline = Ensure-Phase1QueryFocus 'phase1-query-focus'
    Invoke-NativeKeyBatch 'query-focus-shift-space-leader' ([uint16[]]@(0x10, 0x20)) ([string[]]@('Shift', 'Space'))
    $queryActive = Wait-JumpModeObservation {
        param($state)
        $state.active_session -and $state.query -ceq 'note list' -and $state.activation_count -eq ($phase1BaseActivationCount + 1)
    } $CaptureTimeoutSeconds 'query TextEntry target active map'
    $queryTarget = Get-Phase1QueryTarget $queryActive
    $queryTargetId = [string]$queryTarget.target_id
    $queryHint = [string]$queryTarget.hint
    Send-ObservedHint 'query-focus-hint' $queryHint
    $queryFocused = Wait-JumpModeObservation {
        param($state)
        !$state.active_session -and $state.outcome.kind -eq 'completed' -and
            $state.outcome.target_id -ceq $queryTargetId -and $state.focused_destination -ceq $queryTargetId -and
            $state.query_focused -and $state.query -ceq 'note list' -and $state.activation_count -eq ($phase1BaseActivationCount + 1)
    } $CaptureTimeoutSeconds 'actual query TextEntry focused by its observed hint without action execution'
    Write-Event 'phase1_query_focus_handoff' @{
        target_id = $queryTargetId
        hint = $queryHint
        query = $queryFocused.query
        query_focused = $queryFocused.query_focused
        activation_count = $queryFocused.activation_count
        query_before = $queryFocusBaseline.query
    }
    Invoke-NativeKeyBatch 'query-edit-select-all' ([uint16[]]@(0x11, 0x41)) ([string[]]@('Ctrl', 'A'))
    Send-NativeText 'query-edit-x' 'x'
    $queryEdited = Wait-JumpModeObservation {
        param($state)
        !$state.active_session -and $state.query_focused -and $state.query -ceq 'x' -and
            $state.activation_count -eq ($phase1BaseActivationCount + 1)
    } $CaptureTimeoutSeconds 'normal native text editing in the query field after Jump Mode focus handoff'
    Write-Event 'phase1_query_text_edit_observed' @{ query = $queryEdited.query; query_focused = $queryEdited.query_focused; activation_count = $queryEdited.activation_count }
    Invoke-NativeKeyBatch 'query-restore-select-all' ([uint16[]]@(0x11, 0x41)) ([string[]]@('Ctrl', 'A'))
    Send-NativeText 'query-restore-note-list' 'note list'
    $queryRestored = Wait-JumpModeObservation {
        param($state)
        !$state.active_session -and $state.query_focused -and $state.query -ceq 'note list' -and
            $state.result_count -eq 32 -and $state.grid_layout -and $state.activation_count -eq ($phase1BaseActivationCount + 1)
    } $CaptureTimeoutSeconds 'restored 32-note grid after the normal query text edit'
    $queryFocusPredicate = {
        param($state)
        !$state.active_session -and $state.query_focused -and $state.query -ceq 'note list' -and
            $state.result_count -eq 32 -and $state.grid_layout -and $state.activation_count -eq ($phase1BaseActivationCount + 1)
    }
    $null = Save-Phase1Capture 10 'query_focus_and_edit' $queryFocusPredicate 'query hint focused the real TextEntry and native text edit was restored safely'

    Invoke-NativeKeyBatch 'escape-cancel-shift-space-leader' ([uint16[]]@(0x10, 0x20)) ([string[]]@('Shift', 'Space'))
    $escapeActive = Wait-JumpModeObservation {
        param($state)
        $state.active_session -and $state.query -ceq 'note list' -and $state.activation_count -eq ($phase1BaseActivationCount + 1)
    } $CaptureTimeoutSeconds 'Jump Mode active before Escape cancellation'
    $escapeTarget = Get-Phase1Target $escapeActive {
        param($target)
        $target.surface -eq 'launcher_results' -and $null -ne $target.root_action -and $target.hint.Length -ge 2
    } 'a multi-character note result for Escape cancellation'
    Send-ObservedHint 'escape-cancel-prefix' $escapeTarget.hint.Substring(0, 1)
    $escapePartial = Wait-JumpModeObservation {
        param($state)
        $state.active_session -and $state.prefix -ceq $escapeTarget.hint.Substring(0, 1) -and
            $state.prefix -cne $escapeTarget.hint -and $state.prefix_target_ids -contains $escapeTarget.target_id -and
            $state.query -ceq 'note list' -and $state.activation_count -eq ($phase1BaseActivationCount + 1)
    } $CaptureTimeoutSeconds 'partial code before Escape cancellation'
    Write-Event 'phase1_escape_partial_observed' @{ target_id = $escapeTarget.target_id; hint = $escapeTarget.hint; prefix = $escapePartial.prefix; activation_count = $escapePartial.activation_count }
    Invoke-NativeKeyBatch 'escape-cancel-partial-code' ([uint16[]]@(0x1B)) ([string[]]@('Escape'))
    $escapeCancelled = Wait-JumpModeObservation {
        param($state)
        !$state.active_session -and $state.outcome.kind -eq 'cancelled' -and
            $state.query -ceq 'note list' -and $state.activation_count -eq ($phase1BaseActivationCount + 1)
    } $CaptureTimeoutSeconds 'Escape cancelled the partial code without changing query or actions'
    $escapeCancelledPredicate = {
        param($state)
        !$state.active_session -and $state.outcome.kind -eq 'cancelled' -and
            $state.query -ceq 'note list' -and $state.activation_count -eq ($phase1BaseActivationCount + 1)
    }
    $null = Save-Phase1Capture 11 'escape_cancelled_partial_code' $escapeCancelledPredicate 'Escape cancelled a partial code with query and action count preserved'

    Invoke-NativeKeyBatch 'second-leader-start' ([uint16[]]@(0x10, 0x20)) ([string[]]@('Shift', 'Space'))
    $secondLeaderActive = Wait-JumpModeObservation {
        param($state)
        $state.active_session -and $state.prefix -ceq '' -and $state.query -ceq 'note list' -and $state.activation_count -eq ($phase1BaseActivationCount + 1)
    } $CaptureTimeoutSeconds 'Jump Mode active before second-leader cancellation'
    Invoke-NativeKeyBatch 'second-leader-cancel' ([uint16[]]@(0x10, 0x20)) ([string[]]@('Shift', 'Space'))
    $secondLeaderCancelled = Wait-JumpModeObservation {
        param($state)
        !$state.active_session -and $state.outcome.kind -eq 'cancelled' -and
            $state.query -ceq 'note list' -and $state.activation_count -eq ($phase1BaseActivationCount + 1)
    } $CaptureTimeoutSeconds 'second Shift+Space cancelled Jump Mode'
    Write-Event 'phase1_second_leader_cancelled' @{ started_frame = $secondLeaderActive.logic_frame_id; cancelled_frame = $secondLeaderCancelled.logic_frame_id; query = $secondLeaderCancelled.query; activation_count = $secondLeaderCancelled.activation_count }
    $secondLeaderPredicate = {
        param($state)
        !$state.active_session -and $state.outcome.kind -eq 'cancelled' -and
            $state.query -ceq 'note list' -and $state.activation_count -eq ($phase1BaseActivationCount + 1)
    }
    $null = Save-Phase1Capture 12 'second_leader_cancelled' $secondLeaderPredicate 'second Shift+Space cancelled active Jump Mode without actions'

    Invoke-NativeKeyBatch 'dense-map-shift-space-leader' ([uint16[]]@(0x10, 0x20)) ([string[]]@('Shift', 'Space'))
    $denseActive = Wait-JumpModeObservation {
        param($state)
        $resultTargets = @(Get-Phase1ResultTargets $state)
        $state.active_session -and $state.prefix -ceq '' -and $state.query -ceq 'note list' -and
            $state.result_count -eq 32 -and $state.grid_layout -and $resultTargets.Count -gt 26 -and
            $state.activation_count -eq ($phase1BaseActivationCount + 1)
    } $CaptureTimeoutSeconds 'dense active map with more than 26 visible result targets'
    $denseHintTargets = @($denseActive.targets | Where-Object { $_.eligible -and $_.hint })
    Assert-Phase1PrefixFree $denseHintTargets 'dense note grid hint map'
    $denseRootTargetCount = @(Get-Phase1ResultTargets $denseActive).Count
    Write-Event 'phase1_dense_map_observed' @{ eligible_hint_targets = $denseHintTargets.Count; visible_root_result_targets = $denseRootTargetCount; distinct_hints = @($denseHintTargets | ForEach-Object { $_.hint } | Sort-Object -Unique).Count; prefix_free = $true; result_generation = $denseActive.result_generation }
    $denseActivePredicate = {
        param($state)
        $resultTargets = @(Get-Phase1ResultTargets $state)
        $state.active_session -and $state.prefix -ceq '' -and $state.query -ceq 'note list' -and
            $state.result_count -eq 32 -and $state.grid_layout -and $resultTargets.Count -gt 26 -and
            $state.activation_count -eq ($phase1BaseActivationCount + 1)
    }
    $null = Save-Phase1Capture 13 'dense_active_hints_prefix_free' $denseActivePredicate 'more than 26 visible result targets with unique prefix-free hints'
    Invoke-NativeKeyBatch 'dense-map-safe-cancel' ([uint16[]]@(0x1B)) ([string[]]@('Escape'))
    $null = Wait-JumpModeObservation {
        param($state)
        !$state.active_session -and $state.outcome.kind -eq 'cancelled' -and $state.activation_count -eq ($phase1BaseActivationCount + 1)
    } $CaptureTimeoutSeconds 'safe cancellation after dense-map capture'

    if ($phase1Captures.Count -ne $phase1CaptureCount) { throw "Phase 1 enabled $Screen filter saved $($phase1Captures.Count) of $phase1CaptureCount catalog captures" }
    $script:scenarioStatus = 'PASS'
    $script:scenarioReason = "$($phase1Captures.Count) receipt-bound ROOT/F11 captures passed the $Screen filter with actual native Shift+Space, partial/mapped selection, fresh Enter FillQuery and note-open actions, query focus/edit, cancellation, dense prefix-free grid, note-owner blocking, and exact owned cleanup evidence. The capture fixtures keep error notifications enabled."
    $script:runStatus = 'PASS'
    $script:phase = 'phase1_enabled_complete'
    Update-Scenarios
    Write-Event 'phase1_enabled_assertions_passed' @{
        capture_count = $phase1Captures.Count
        screen = $Screen
        dashboard_fill_query_activations = if ($Screen -eq 'Grid') { 0 } else { 1 }
        note_open_activations = 1
        total_activation_count = $phase1BaseActivationCount + 1
        stale_target_enter_activation_count = $phase1BaseActivationCount
        opened_note_slug = 'jump-qa-note-001'
        query = 'note list'
    }
    Write-Summary 'PASS' "Enabled Phase 1 $Screen filter and its receipt-bound retained states passed. The app will close through its owned window, followed by bounded process-tree cleanup verification."
}

function Invoke-Phase1DisabledWorkflow {
    Write-Event 'phase1_disabled_native_sequence_started' @{ expected_capture_count = 1; jump_mode_enabled = $false }
    $baseline = Wait-JumpModeObservation {
        param($state)
        $state.root_focused -and !$state.jump_mode_enabled -and
            !$state.active_session -and $state.query -ceq '' -and $state.activation_count -eq 0
    } $CaptureTimeoutSeconds 'disabled-gate isolated ROOT baseline'
    $baseline = Ensure-Phase1QueryFocus 'phase1-disabled-query' $false
    $baselineFrame = [int64]$baseline.logic_frame_id
    Invoke-NativeKeyBatch 'disabled-gate-shift-space' ([uint16[]]@(0x10, 0x20)) ([string[]]@('Shift', 'Space'))
    $ordinarySpace = Wait-JumpModeObservation {
        param($state)
        [int64]$state.logic_frame_id -gt $baselineFrame -and !$state.jump_mode_enabled -and
            !$state.active_session -and $state.query_focused -and $state.query -ceq ' ' -and
            $state.activation_count -eq 0
    } $CaptureTimeoutSeconds 'ordinary Shift+Space text input with Jump Mode disabled'
    Invoke-NativeKeyBatch 'disabled-gate-normal-text-x' ([uint16[]]@([char]'X')) ([string[]]@('X'))
    $ordinaryText = Wait-JumpModeObservation {
        param($state)
        !$state.jump_mode_enabled -and !$state.active_session -and $state.query_focused -and
            $state.query -ceq ' x' -and $state.activation_count -eq 0
    } $CaptureTimeoutSeconds 'ordinary native text input after disabled Shift+Space'
    Write-Event 'phase1_disabled_ordinary_input_observed' @{
        query_after_shift_space = $ordinarySpace.query
        query_after_text = $ordinaryText.query
        jump_mode_enabled = $ordinaryText.jump_mode_enabled
        active_session = $ordinaryText.active_session
        activation_count = $ordinaryText.activation_count
        registered_targets = @($ordinaryText.targets).Count
    }
    $disabledPredicate = {
        param($state)
        $state.root_focused -and !$state.active_session -and !$state.jump_mode_enabled -and
            $state.query -ceq ' x' -and $state.query_focused -and $state.activation_count -eq 0 -and
            @($state.targets).Count -eq 0 -and $null -eq $state.query_target_id
    }
    $null = Save-Phase1Capture 18 'disabled_shift_space_preserves_normal_text' $disabledPredicate 'disabled gate leaves Shift+Space and following text in the focused query field'
    $script:scenarioStatus = 'PASS'
    $script:scenarioReason = 'Separate disabled fixture kept Jump Mode inactive and registered no Jump targets; native Shift+Space inserted a normal space, following X edited the query, and action count stayed zero. The actual ROOT/F11 receipt is bound to the disabled fixture and retained frame.'
    $script:runStatus = 'PASS'
    $script:phase = 'phase1_disabled_complete'
    Update-Scenarios
    Write-Event 'phase1_disabled_assertions_passed' @{ capture_count = 1; jump_mode_enabled = $false; query = $ordinaryText.query; activation_count = $ordinaryText.activation_count }
    Write-Summary 'PASS' 'Disabled-gate native regression and its receipt-bound ROOT/F11 capture passed. The app will close through its owned window, followed by bounded process-tree cleanup verification.'
}

function Complete-ProcessLogs {
    if (!$process) { return }
    try {
        if ($stdoutTask -and $stdoutTask.IsCompleted) {
            [IO.File]::WriteAllText($stdoutPath, $stdoutTask.GetAwaiter().GetResult(), [Text.UTF8Encoding]::new($false))
        }
        if ($stderrTask -and $stderrTask.IsCompleted) {
            [IO.File]::WriteAllText($stderrPath, $stderrTask.GetAwaiter().GetResult(), [Text.UTF8Encoding]::new($false))
        }
    } catch {
        Write-CleanupEvent 'process_log_persist_error' @{ message = $_.Exception.Message }
    }
}

function Get-ProcessCreationUtc($CimProcess) {
    if ($CimProcess.CreationDate -is [string]) {
        return [Management.ManagementDateTimeConverter]::ToDateTime($CimProcess.CreationDate).ToUniversalTime()
    }
    return ([DateTime]$CimProcess.CreationDate).ToUniversalTime()
}

function Get-OwnedTreeSnapshot {
    $allProcesses = @(Get-CimInstance -ClassName Win32_Process)
    $root = $allProcesses | Where-Object { [int]$_.ProcessId -eq $processId } | Select-Object -First 1
    if (!$root) { return @() }
    $observedRootStart = Get-ProcessCreationUtc $root
    if ([Math]::Abs(($observedRootStart - $processStartUtc).Ticks) -gt 100) {
        throw 'process-table root creation time does not match the retained process handle'
    }
    $tree = [Collections.Generic.List[object]]::new()
    $tree.Add([ordered]@{
        pid = [int]$root.ProcessId
        parent_pid = [int]$root.ParentProcessId
        creation_time_utc = $observedRootStart.ToString('O')
        image = [string]$root.Name
    })
    $seen = [Collections.Generic.HashSet[int]]::new()
    $creationByPid = [Collections.Generic.Dictionary[int, DateTime]]::new()
    $null = $seen.Add([int]$root.ProcessId)
    $creationByPid.Add([int]$root.ProcessId, $observedRootStart)
    do {
        $added = $false
        foreach ($candidate in $allProcesses) {
            $candidatePid = [int]$candidate.ProcessId
            $parentPid = [int]$candidate.ParentProcessId
            if (!$seen.Contains($parentPid) -or $seen.Contains($candidatePid)) { continue }
            $candidateCreation = Get-ProcessCreationUtc $candidate
            if ($candidateCreation -lt $creationByPid[$parentPid]) { continue }
            $null = $seen.Add($candidatePid)
            $creationByPid.Add($candidatePid, $candidateCreation)
            $tree.Add([ordered]@{
                pid = $candidatePid
                parent_pid = $parentPid
                creation_time_utc = $candidateCreation.ToString('O')
                image = [string]$candidate.Name
            })
            $added = $true
        }
    } while ($added)
    return $tree.ToArray()
}

function Merge-OwnedTreeSnapshot($NewProcesses) {
    $known = [Collections.Generic.HashSet[string]]::new()
    foreach ($owner in $script:ownedTreeSnapshot) {
        $null = $known.Add("$($owner.pid):$($owner.creation_time_utc)")
    }
    foreach ($owner in $NewProcesses) {
        $key = "$($owner.pid):$($owner.creation_time_utc)"
        if ($known.Add($key)) { $script:ownedTreeSnapshot += $owner }
    }
}

function Test-OwnedTreeGone {
    foreach ($owner in $ownedTreeSnapshot) {
        $liveProcess = Get-CimInstance -ClassName Win32_Process -Filter "ProcessId = $($owner.pid)"
        if (!$liveProcess) { continue }
        $liveCreation = (Get-ProcessCreationUtc $liveProcess).ToString('O')
        if ($liveCreation -eq $owner.creation_time_utc) { return $false }
    }
    return $true
}

function Test-RetainedProcessIdentity {
    try {
        $process.Refresh()
        return $process.Id -eq $processId -and
            [Math]::Abs(($process.StartTime.ToUniversalTime() - $processStartUtc).TotalMilliseconds) -le 5
    } catch {
        return $false
    }
}

function Close-OwnedProcess {
    if (!$process) { return }

    # Process-table or evidence failures must never bypass the retained-handle
    # shutdown path. Record inventory uncertainty first, then independently
    # attempt graceful close and the exact-owned-process fallback.
    try {
        $process.Refresh()
        $snapshot = @(Get-OwnedTreeSnapshot)
        if ($snapshot.Count -eq 0) { throw 'no owned process tree snapshot was captured before shutdown' }
        Merge-OwnedTreeSnapshot $snapshot
        Write-CleanupEvent 'owned_process_tree_observed' @{ processes = $script:ownedTreeSnapshot }
    } catch {
        Record-CleanupIssue 'process inventory' $_.Exception.Message
    }

    try {
        $process.Refresh()
        if (!$process.HasExited) {
            if (!(Test-RetainedProcessIdentity)) {
                Record-CleanupIssue 'graceful close' 'retained process identity could not be verified'
            } else {
                try {
                    $script:gracefulCloseRequested = $process.CloseMainWindow()
                    Write-CleanupEvent 'graceful_close_requested' @{
                        pid = $processId
                        start_time_utc = $processStartUtc.ToString('O')
                        close_main_window_returned = $script:gracefulCloseRequested
                        wait_seconds = $GracefulCloseSeconds
                    }
                } catch {
                    Record-CleanupIssue 'graceful close' $_.Exception.Message
                }
                try { $null = $process.WaitForExit($GracefulCloseSeconds * 1000) }
                catch { Record-CleanupIssue 'graceful wait' $_.Exception.Message }
            }
        }
    } catch {
        Record-CleanupIssue 'graceful close' $_.Exception.Message
    }

    try {
        $process.Refresh()
        if (!$process.HasExited) {
            if (!(Test-RetainedProcessIdentity)) {
                Record-CleanupIssue 'owned-process fallback' 'refusing forced cleanup because the retained process identity could not be verified'
            } else {
                try {
                    $process.Kill($true)
                    $script:forcedTreeCleanup = $true
                    $null = $process.WaitForExit(5000)
                    Write-CleanupEvent 'owned_tree_cleanup_requested' @{ pid = $processId; method = 'retained-process-handle Kill(entireProcessTree=true)' }
                } catch {
                    Record-CleanupIssue 'owned-process fallback' $_.Exception.Message
                }
            }
        }
    } catch {
        Record-CleanupIssue 'owned-process fallback' $_.Exception.Message
    }

    try {
        $process.Refresh()
        if ($process.HasExited) {
            $script:processExitCode = $process.ExitCode
        } else {
            Record-CleanupIssue 'owned-process cleanup' 'verified root process remains alive after bounded cleanup'
            $script:postCleanupState = 'root_still_alive'
        }
    } catch {
        Record-CleanupIssue 'owned-process exit check' $_.Exception.Message
    }

    try {
        if ($script:processExitCode -ne $null) {
            if ($script:ownedTreeSnapshot.Count -eq 0) {
                Record-CleanupIssue 'descendant verification' 'the owned process tree could not be inventoried before shutdown'
            } else {
                $treeDeadline = [DateTime]::UtcNow.AddSeconds(5)
                do {
                    try {
                        if (Test-OwnedTreeGone) {
                            $script:ownedTreeVerified = $true
                            if ($script:cleanupIssues.Count -eq 0) { $script:postCleanupState = 'owned_tree_gone' }
                            break
                        }
                    } catch {
                        Record-CleanupIssue 'descendant verification' $_.Exception.Message
                        break
                    }
                    Start-Sleep -Milliseconds 200
                } while ([DateTime]::UtcNow -lt $treeDeadline)
                if (!$script:ownedTreeVerified -and $script:postCleanupState -ne 'cleanup_uncertain') {
                    Record-CleanupIssue 'descendant verification' 'one or more identified owned descendants remain alive'
                    $script:postCleanupState = 'owned_tree_still_alive'
                }
            }
        }
    } catch {
        Record-CleanupIssue 'descendant verification' $_.Exception.Message
    } finally {
        try { Complete-ProcessLogs } catch { Record-CleanupIssue 'log persistence' $_.Exception.Message }
        try { $process.Dispose() } catch { Record-CleanupIssue 'retained process handle disposal' $_.Exception.Message }
    }
}

function Update-Scenarios {
    $fixtureScenario = [ordered]@{
        scenario_id = switch ($Scenario) {
            'QueryProbe' { 'm05-fixture-validation' }
            { $_ -in @('Phase1', 'Phase1Enabled', 'Phase1Disabled') } { 'm08-fixture-validation' }
            default { 'm01-fixture-validation' }
        }
        status = $fixtureScenarioStatus
        expected_surface = if ($Scenario -in @('Phase1', 'Phase1Enabled', 'Phase1Disabled')) { 'typed isolated safe Phase 1 enabled/disabled fixture profiles' } else { 'typed isolated fixture profile with stable safe note data' }
        capture_target = $null
        capture_trigger = $null
        screenshot = $null
        pixel_width = $null
        pixel_height = $null
        reason = $fixtureScenarioReason
    }
    $runScenario = [ordered]@{
        scenario_id = switch ($Scenario) {
            'QueryProbe' { 'm05-query-only' }
            { $_ -in @('Phase1', 'Phase1Enabled', 'Phase1Disabled') } {
                $suffix = if ($Scenario -eq 'Phase1Enabled' -and $Screen -ne 'All') { "-$($Screen.ToLowerInvariant())" } else { '' }
                "m08-$($Scenario.ToLowerInvariant())$suffix"
            }
            default { 'm01-baseline-dashboard' }
        }
        status = $scenarioStatus
        expected_surface = switch ($Scenario) {
            'QueryProbe' { 'active Jump Mode query hint, query focus handoff, and unchanged action count' }
            { $_ -in @('Phase1', 'Phase1Enabled', 'Phase1Disabled') } { "native Phase 1 $Screen dashboard/grid filter, selection, cancellation, note owner, and disabled-gate behavior" }
            default { 'actual Multi Launcher ROOT viewport with isolated note dashboard' }
        }
        capture_target = 'ROOT'
        capture_trigger = if ($Scenario -in @('Phase1Enabled', 'Phase1Disabled')) { 'f11' } elseif ($Scenario -eq 'Phase1') { 'child native F11 receipts' } else { 'startup' }
        screenshot = if ($screenshotPath) { $screenshotPath } else { $null }
        pixel_width = $screenshotWidth
        pixel_height = $screenshotHeight
        captures = @($phase1Captures.ToArray())
        orchestration = $phase1Orchestration
        reason = $scenarioReason
    }
    Write-Json $scenariosPath @($fixtureScenario, $runScenario)
}

function Invoke-Phase1Orchestration {
    $childResults = [Collections.Generic.List[object]]::new()
    $allCaptures = [Collections.Generic.List[object]]::new()
    $script:phase1Orchestration = [ordered]@{
        schema_version = 1
        scenario = 'Phase1'
        children = $childResults
        captures = $allCaptures
    }
    $phase1IndexPath = Join-Path $evidenceRoot 'phase1-index.json'
    Write-Json $phase1IndexPath $phase1Orchestration
    if (!$interactive -or $sessionId -eq 0) {
        $script:scenarioStatus = 'NOT_RUN_ENVIRONMENT'
        $script:scenarioReason = 'Windows reports no interactive user session; no child app was launched and no screenshot was fabricated.'
        $script:runStatus = 'NOT_RUN_ENVIRONMENT'
        $script:phase = 'phase1_not_run_environment'
        Write-Event 'phase1_not_run_environment' @{ user_interactive = $interactive; session_id = $sessionId }
        Write-Summary 'NOT_RUN_ENVIRONMENT' 'The enabled and disabled Phase 1 app runs were skipped because this process is not in an interactive Windows user session; no screenshots were fabricated.'
        Update-Scenarios
        return
    }
    $expectedByScenario = @{
        Phase1Enabled = @(1..14)
        Phase1Disabled = @(18)
    }
    $pwshPath = Join-Path $PSHOME 'pwsh.exe'
    foreach ($childScenario in @('Phase1Enabled', 'Phase1Disabled')) {
        $childLogPath = Join-Path $evidenceRoot "$($childScenario.ToLowerInvariant()).stdout.log"
        $childRecord = [ordered]@{
            scenario = $childScenario
            status = 'FAIL'
            exit_code = $null
            evidence = $null
            manifest_status = $null
            process_lifecycle = $null
            captures = @()
            error = $null
            output_log = [IO.Path]::GetFileName($childLogPath)
        }
        try {
            Write-Event 'phase1_child_started' @{ scenario = $childScenario }
            $remainingChildBudget = Get-RemainingRunSeconds $OverallTimeoutSeconds
            if ($remainingChildBudget -lt 330) { throw 'less than 330 seconds remain for the next bounded child run and its cleanup margin' }
            $childBudget = $remainingChildBudget - 30
            $childErrorLogPath = Join-Path $evidenceRoot "$($childScenario.ToLowerInvariant()).stderr.log"
            $childResult = Invoke-BoundedCommand $pwshPath @(
                '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $PSCommandPath,
                '-Scenario', $childScenario, '-Screen', 'All',
                '-CaptureTimeoutSeconds', [string]$CaptureTimeoutSeconds,
                '-GracefulCloseSeconds', [string]$GracefulCloseSeconds,
                '-OverallTimeoutSeconds', [string]$childBudget
            ) $repoRoot $childLogPath $childErrorLogPath "Phase1 $childScenario child run"
            $childExitCode = $childResult.ExitCode
            $childOutputText = $childResult.StandardOutput + [Environment]::NewLine + $childResult.StandardError
            $childOutput = @($childOutputText -split '\r?\n')
            [IO.File]::WriteAllText($childLogPath, $childResult.StandardOutput + $childResult.StandardError, [Text.UTF8Encoding]::new($false))
            $childRecord.exit_code = $childExitCode
            $evidenceLines = @($childOutput | ForEach-Object { [string]$_ } | Where-Object { $_ -match '^Evidence: ' })
            if ($evidenceLines.Count -ne 1) { throw "child emitted $($evidenceLines.Count) Evidence paths" }
            $childEvidence = [IO.Path]::GetFullPath($evidenceLines[0].Substring('Evidence: '.Length).Trim())
            $expectedEvidenceRoot = [IO.Path]::GetFullPath((Join-Path $repoRoot 'dev\jump_mode\evidence'))
            if (!$childEvidence.StartsWith($expectedEvidenceRoot + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
                throw 'child evidence path escaped the jump-mode evidence directory'
            }
            $childRecord.evidence = $childEvidence
            $childManifestPath = Join-Path $childEvidence 'manifest.json'
            $childScenariosPath = Join-Path $childEvidence 'scenarios.json'
            $childLifecyclePath = Join-Path $childEvidence 'process_lifecycle.json'
            if (!(Test-Path -LiteralPath $childManifestPath -PathType Leaf) -or
                !(Test-Path -LiteralPath $childScenariosPath -PathType Leaf) -or
                !(Test-Path -LiteralPath $childLifecyclePath -PathType Leaf)) {
                throw 'child evidence is missing its manifest, scenarios, or process lifecycle record'
            }
            $childManifest = Get-Content -Raw -LiteralPath $childManifestPath | ConvertFrom-Json
            $childScenarios = @(Get-Content -Raw -LiteralPath $childScenariosPath | ConvertFrom-Json)
            $childLifecycle = Get-Content -Raw -LiteralPath $childLifecyclePath | ConvertFrom-Json
            $childRecord.manifest_status = [string]$childManifest.status
            $childRecord.environment = $childManifest.environment
            $childRecord.process_lifecycle = [ordered]@{
                pid = $childLifecycle.pid
                post_cleanup_state = $childLifecycle.post_cleanup_state
                owned_tree_verified = $childLifecycle.owned_tree_verified
                exit_code = $childLifecycle.exit_code
            }
            if ($childExitCode -ne 0 -or $childManifest.status -ne 'PASS') { throw "child run failed (exit=$childExitCode, manifest=$($childManifest.status))" }
            if ($childLifecycle.post_cleanup_state -ne 'owned_tree_gone' -or !$childLifecycle.owned_tree_verified) {
                throw 'child process lifecycle does not prove the owned process tree is gone'
            }
            $childScenarioResult = @($childScenarios | Where-Object { $_.scenario_id -eq "m08-$($childScenario.ToLowerInvariant())" })
            if ($childScenarioResult.Count -ne 1 -or $childScenarioResult[0].status -ne 'PASS') { throw 'child Phase 1 scenario is not marked PASS' }
            $expectedCatalogNumbers = @($expectedByScenario[$childScenario])
            $childCaptures = @($childScenarioResult[0].captures)
            $actualCatalogNumbers = @($childCaptures | ForEach-Object { [int]$_.catalog_number } | Sort-Object -Unique)
            if ($childCaptures.Count -ne $expectedCatalogNumbers.Count -or
                (($actualCatalogNumbers -join ',') -cne (($expectedCatalogNumbers | Sort-Object) -join ','))) {
                throw "child captured catalog entries [$($actualCatalogNumbers -join ',')] instead of [$($expectedCatalogNumbers -join ',')]"
            }
            foreach ($capture in $childCaptures) {
                if ($capture.status -ne 'PASS') { throw "catalog $($capture.catalog_number) is not marked PASS" }
                $sourceScreenshot = [IO.Path]::GetFullPath((Join-Path $childEvidence ([string]$capture.screenshot).Replace('/', [IO.Path]::DirectorySeparatorChar)))
                if (!$sourceScreenshot.StartsWith($childEvidence + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase) -or
                    !(Test-Path -LiteralPath $sourceScreenshot -PathType Leaf)) { throw "catalog $($capture.catalog_number) screenshot is missing or escaped the child evidence directory" }
                $sourceHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $sourceScreenshot).Hash.ToLowerInvariant()
                if ($sourceHash -cne [string]$capture.screenshot_sha256) { throw "catalog $($capture.catalog_number) screenshot hash does not match its child scenario record" }
                $receiptPath = [IO.Path]::GetFullPath((Join-Path $childEvidence ([string]$capture.receipt).Replace('/', [IO.Path]::DirectorySeparatorChar)))
                if (!$receiptPath.StartsWith($childEvidence + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase) -or
                    !(Test-Path -LiteralPath $receiptPath -PathType Leaf)) { throw "catalog $($capture.catalog_number) native receipt is missing or escaped the child evidence directory" }
                $receipt = Get-Content -Raw -LiteralPath $receiptPath | ConvertFrom-Json
                if ($receipt.status -ne 'saved' -or [int]$receipt.process.pid -ne [int]$capture.process_pid -or
                    [string]$receipt.executable.sha256 -cne [string]$capture.executable_sha256 -or
                    [string]$receipt.fixture_manifest_sha256 -cne [string]$capture.fixture_manifest_sha256 -or
                    [string]$receipt.saved.png_sha256 -cne $sourceHash -or
                    [int64]$receipt.captured.logic_frame_id -ne [int64]$capture.logic_frame_id -or
                    [int64]$receipt.captured.ui_pass_id -ne [int64]$capture.ui_pass_id) {
                    throw "catalog $($capture.catalog_number) receipt is not bound to the scenario's exact captured PNG and retained frame"
                }
                $destinationName = [IO.Path]::GetFileName($sourceScreenshot)
                $destinationPath = Join-Path $screenshotsRoot $destinationName
                if (Test-Path -LiteralPath $destinationPath) { throw "aggregate catalog screenshot already exists: $destinationName" }
                Copy-Item -LiteralPath $sourceScreenshot -Destination $destinationPath
                $copiedHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $destinationPath).Hash.ToLowerInvariant()
                if ($copiedHash -cne $sourceHash) { throw "catalog $($capture.catalog_number) aggregate screenshot copy changed bytes" }
                $aggregateCapture = [ordered]@{
                    catalog_number = [int]$capture.catalog_number
                    name = [string]$capture.name
                    status = 'PASS'
                    screenshot = "screenshots/$destinationName"
                    screenshot_sha256 = $sourceHash
                    source_scenario = $childScenario
                    source_evidence = $childEvidence
                    source_receipt = [string]$capture.receipt
                    process_pid = [int]$capture.process_pid
                    owned_tree_gone = $true
                    fixture_manifest_sha256 = [string]$capture.fixture_manifest_sha256
                    logic_frame_id = [int64]$capture.logic_frame_id
                    ui_pass_id = [int64]$capture.ui_pass_id
                    query = [string]$capture.query
                    result_generation = $capture.result_generation
                    result_count = $capture.result_count
                    selected_index = $capture.selected_index
                    selected_target_id = $capture.selected_target_id
                    activation_count = [int]$capture.activation_count
                    active_session = [bool]$capture.active_session
                    prefix = [string]$capture.prefix
                    pixels_per_point = $capture.pixels_per_point
                    native_pixels_per_point = $capture.native_pixels_per_point
                    zoom_factor = $capture.zoom_factor
                }
                $allCaptures.Add($aggregateCapture)
                $script:phase1Captures.Add($aggregateCapture)
                $png = Get-PngDimensions $destinationPath
                $script:screenshotPath = $aggregateCapture.screenshot
                $script:screenshotWidth = $png.Width
                $script:screenshotHeight = $png.Height
                Write-Json $phase1IndexPath $phase1Orchestration
                Update-Scenarios
                Write-Event 'phase1_aggregate_capture_bound' $aggregateCapture
            }
            $childRecord.captures = @($childCaptures | ForEach-Object { [ordered]@{ catalog_number = [int]$_.catalog_number; screenshot = [string]$_.screenshot; screenshot_sha256 = [string]$_.screenshot_sha256; logic_frame_id = [int64]$_.logic_frame_id; ui_pass_id = [int64]$_.ui_pass_id } })
            $childRecord.status = 'PASS'
        } catch {
            $childRecord.error = $_.Exception.Message
            if (!(Test-Path -LiteralPath $childLogPath -PathType Leaf)) {
                [IO.File]::WriteAllText($childLogPath, [string]$_.Exception.Message, [Text.UTF8Encoding]::new($false))
            }
        }
        $childResults.Add($childRecord)
        Write-Json $phase1IndexPath $phase1Orchestration
        Update-Scenarios
        Write-Event 'phase1_child_completed' $childRecord
    }

    $enabledResult = @($childResults | Where-Object { $_.scenario -eq 'Phase1Enabled' -and $_.status -eq 'PASS' }).Count -eq 1
    $disabledResult = @($childResults | Where-Object { $_.scenario -eq 'Phase1Disabled' -and $_.status -eq 'PASS' }).Count -eq 1
    if ($enabledResult -and $disabledResult -and $allCaptures.Count -eq 15) {
        $script:scenarioStatus = 'PASS'
        $script:fixtureScenarioStatus = 'PASS'
        $script:fixtureScenarioReason = 'Both child fixtures passed typed semantic validation, including the invalid-gate negative proof with Paste Sequence Next and Cancel disabled.'
        $script:scenarioReason = 'Fourteen enabled native Phase 1 cases and the separate disabled-gate regression passed. All fifteen catalog screenshots are copied from native ROOT/F11 captures bound to their exact receipt, retained frame/pass, process identity, fixture hash, and verified owned cleanup.'
        $script:runStatus = 'PASS'
        $script:phase = 'phase1_complete'
        Write-Event 'phase1_orchestration_passed' @{ capture_count = $allCaptures.Count; child_scenarios = @('Phase1Enabled', 'Phase1Disabled') }
        Write-Summary 'PASS' 'Enabled Phase 1 catalog 01–14 and disabled-gate case 18 passed as separate owned app runs. All screenshots are verified native ROOT/F11 pixels bound to matching retained snapshots and saved receipts; both process trees were verified gone.'
    } else {
        $script:scenarioStatus = 'FAIL'
        $script:fixtureScenarioStatus = 'FAIL'
        $script:failureText = 'Phase 1 child scenarios did not both pass with all fifteen receipt-bound catalog images and owned cleanup evidence.'
        $script:scenarioReason = $script:failureText
        $script:fixtureScenarioReason = 'See child fixture validation logs and phase1-index.json.'
        $script:runStatus = 'FAIL'
        $script:phase = 'phase1_failed'
        Write-Event 'phase1_orchestration_failed' @{ capture_count = $allCaptures.Count; children = @($childResults.ToArray()) }
        Write-Summary 'FAIL' $script:failureText
    }
    Update-Scenarios
}

try {
    if ($Screen -ne 'All' -and $Scenario -ne 'Phase1Enabled') { throw 'Dashboard and Grid filters are available only for the enabled Phase 1 scenario' }
    if (Test-Path -LiteralPath $evidenceRoot) { throw "refusing to reuse evidence path $evidenceRoot" }
    New-Item -ItemType Directory -Path $evidenceRoot | Out-Null
    New-Item -ItemType Directory -Path $screenshotsRoot | Out-Null
    [IO.File]::WriteAllText($eventsPath, '', [Text.UTF8Encoding]::new($false))
    [IO.File]::WriteAllText($stdoutPath, '', [Text.UTF8Encoding]::new($false))
    [IO.File]::WriteAllText($stderrPath, '', [Text.UTF8Encoding]::new($false))
    Write-Json $manifestPath ([ordered]@{
        schema_version = 1
        run_id = $runId
        milestone = if ($Scenario -eq 'QueryProbe') { 'M05' } elseif ($Scenario -in @('Phase1', 'Phase1Enabled', 'Phase1Disabled')) { 'M08' } else { 'M01' }
        status = $runStatus
        phase = $phase
        started_at_utc = $startedAt.ToString('O')
        repository_root = $repoRoot
        source_head = $null
        interactive_session = $interactive
        session_id = $sessionId
        screen_filter = $Screen
        overall_timeout_seconds = $OverallTimeoutSeconds
        environment = [ordered]@{ os_version = [Environment]::OSVersion.VersionString; architecture = [Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString(); interactive_session = $interactive; session_id = $sessionId; session_name = $env:SESSIONNAME; window = $null }
        fixture_profile = 'profile'
        fixture_manifest_sha256 = $null
        executable = $null
        build_receipt_identity = $null
        capture = $null
        cleanup = $null
    })
    Update-Scenarios
    $fixtureScenarioArgument = switch ($Scenario) {
        'QueryProbe' { 'query-probe' }
        'Phase1Enabled' { 'phase1-enabled' }
        'Phase1Disabled' { 'phase1-disabled' }
        default { 'baseline' }
    }
    Write-Summary 'BLOCKED' "Runner is preparing a fresh $Scenario fixture and validating its typed production semantics before launch."
    Write-Event 'run_created' @{ run_id = $runId; profile = $profileRoot; interactive = $interactive; session_id = $sessionId; overall_timeout_seconds = $OverallTimeoutSeconds; screen_filter = $Screen }

    if ($Scenario -eq 'Phase1' -and $FixturesOnly) {
        throw 'The Phase1 aggregate orchestrates native GUI child runs and cannot be fixture-only. Validate children separately with -Scenario Phase1Enabled -FixturesOnly and -Scenario Phase1Disabled -FixturesOnly.'
    }

    if ($Scenario -eq 'Phase1') {
        $phase = 'phase1_children'
        Write-Summary 'BLOCKED' 'Running separate enabled and disabled isolated Phase 1 child scenarios with native ROOT/F11 capture and verified owned cleanup.'
        Invoke-Phase1Orchestration
    } else {
    $cargoExecutable = (Get-Command cargo.exe -ErrorAction Stop).Source
    $metadataResult = Invoke-BoundedCommand $cargoExecutable @('metadata', '--no-deps', '--format-version', '1', '--manifest-path', (Join-Path $repoRoot 'Cargo.toml')) $repoRoot (Join-Path $evidenceRoot 'metadata.stdout.log') (Join-Path $evidenceRoot 'metadata.stderr.log') 'Cargo metadata'
    if ($metadataResult.ExitCode -ne 0) { throw 'cargo metadata failed; see metadata logs' }
    $metadata = $metadataResult.StandardOutput | ConvertFrom-Json
    $targetDirectory = [IO.Path]::GetFullPath([string]$metadata.target_directory)
    $env:CARGO_TARGET_DIR = $targetDirectory
    $headSha = (& git -C $repoRoot rev-parse HEAD).Trim()
    if ($LASTEXITCODE -ne 0) { throw 'git rev-parse HEAD failed' }

    $phase = 'fixture_validation'
    Write-Event 'fixture_tests_started' @{ manifest = (Join-Path $repoRoot 'Cargo.toml') }
    $fixtureTestResult = Invoke-BoundedCommand $cargoExecutable @('test', '--manifest-path', (Join-Path $repoRoot 'Cargo.toml'), '--profile', 'fast-dev', '--features', $cargoFeatures, '--bin', 'jump_mode_fixture') $repoRoot $testStdoutPath $testStderrPath 'jump mode fixture tests'
    $fixtureTestsExitCode = $fixtureTestResult.ExitCode
    Write-Event 'fixture_tests_completed' @{ exit_code = $fixtureTestsExitCode }
    if ($fixtureTestsExitCode -ne 0) { throw 'fixture generator tests failed; see fixture-tests logs' }

    Write-Event 'fixture_generation_started' @{ profile = $profileRoot }
    $fixtureGenerateResult = Invoke-BoundedCommand $cargoExecutable @('run', '--quiet', '--manifest-path', (Join-Path $repoRoot 'Cargo.toml'), '--profile', 'fast-dev', '--features', $cargoFeatures, '--bin', 'jump_mode_fixture', '--', 'generate', $profileRoot, $fixtureScenarioArgument) $repoRoot $fixtureStdoutPath $fixtureStderrPath 'jump mode fixture generation'
    $fixtureGenerateExitCode = $fixtureGenerateResult.ExitCode
    if ($fixtureGenerateExitCode -ne 0) { throw 'fixture generation failed; see fixture logs' }
    $fixtureVerifyResult = Invoke-BoundedCommand $cargoExecutable @('run', '--quiet', '--manifest-path', (Join-Path $repoRoot 'Cargo.toml'), '--profile', 'fast-dev', '--features', $cargoFeatures, '--bin', 'jump_mode_fixture', '--', 'verify', $profileRoot, $fixtureScenarioArgument) $repoRoot (Join-Path $evidenceRoot 'fixture-verify.stdout.log') (Join-Path $evidenceRoot 'fixture-verify.stderr.log') 'jump mode fixture verification'
    $fixtureVerifyExitCode = $fixtureVerifyResult.ExitCode
    Write-Event 'fixture_generation_completed' @{ generate_exit_code = $fixtureGenerateExitCode; verify_exit_code = $fixtureVerifyExitCode }
    if ($fixtureVerifyExitCode -ne 0) { throw 'fixture verification failed; see fixture logs' }
    if ($Scenario -eq 'Phase1Enabled') {
        $invalidProfileRoot = Join-Path $evidenceRoot 'invalid-gate-profile'
        Copy-Item -LiteralPath $profileRoot -Destination $invalidProfileRoot -Recurse
        $invalidSettingsPath = Join-Path $invalidProfileRoot 'settings.json'
        $invalidSettings = Get-Content -Raw -LiteralPath $invalidSettingsPath | ConvertFrom-Json
        $invalidSettings.experimental_jump_mode_enabled = $false
        [IO.File]::WriteAllText(
            $invalidSettingsPath,
            (ConvertTo-Json -InputObject $invalidSettings -Depth 64) + [Environment]::NewLine,
            [Text.UTF8Encoding]::new($false)
        )
        $negativeStdoutPath = Join-Path $evidenceRoot 'invalid-fixture.stdout.log'
        $negativeStderrPath = Join-Path $evidenceRoot 'invalid-fixture.stderr.log'
        $negativeFixtureResult = Invoke-BoundedCommand $cargoExecutable @('run', '--quiet', '--manifest-path', (Join-Path $repoRoot 'Cargo.toml'), '--profile', 'fast-dev', '--features', $cargoFeatures, '--bin', 'jump_mode_fixture', '--', 'verify', $invalidProfileRoot, $fixtureScenarioArgument) $repoRoot $negativeStdoutPath $negativeStderrPath 'invalid fixture rejection check'
        $negativeFixtureExitCode = $negativeFixtureResult.ExitCode
        $negativeFixtureOutput = (Get-Content -Raw -LiteralPath $negativeStdoutPath) + (Get-Content -Raw -LiteralPath $negativeStderrPath)
        if ($negativeFixtureExitCode -eq 0 -or !$negativeFixtureOutput.Contains('fixture Jump Mode setting does not match the requested scenario')) {
            throw 'the intentionally invalid Jump Mode gate profile was not rejected by the fixture verifier'
        }
        Write-Event 'invalid_fixture_rejected' @{ scenario = 'jump-mode-gate-mismatch'; exit_code = $negativeFixtureExitCode; reason = 'fixture Jump Mode setting does not match the requested scenario'; paste_sequence_next_hotkey = $invalidSettings.paste_sequence.next_hotkey; paste_sequence_cancel_hotkey = $invalidSettings.paste_sequence.cancel_hotkey }
    }
    $fixtureManifestFile = Join-Path $profileRoot 'fixture_manifest.json'
    $fixtureManifest = Get-Content -Raw -LiteralPath $fixtureManifestFile | ConvertFrom-Json
    $staleInvalidationQuery = [string]$fixtureManifest.no_results_query
    if ([string]::IsNullOrWhiteSpace($staleInvalidationQuery)) { throw 'verified fixture manifest has no production-checked no-results query' }
    $fixtureHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $fixtureManifestFile).Hash.ToLowerInvariant()
    if ($Scenario -in @('Phase1Enabled', 'Phase1Disabled')) { Test-Phase1CapturePredicateComposition }
    $fixtureScenarioStatus = 'PASS'
    $fixtureScenarioReason = 'Typed settings/dashboard and production pinned-query config passed; isolated production note loading/search returned all 32 stable note IDs, four underscore bucket tags, and zero actions for the manifest no-results query; the production hotkey-route validator accepted the isolated fixture chord; side-effect safety checks passed.'
    if ($Scenario -eq 'Phase1Enabled') {
        $fixtureScenarioReason += ' The negative Jump Mode gate profile was rejected for the expected fixture invariant; Paste Sequence Next and Cancel remained disabled.'
    }
    $phase = 'fixture_checked'
    Update-Scenarios
    Write-Summary 'BLOCKED' 'Fixture validation passed typed production widget config and production note loader/search checks: 32 list results, 32 search results, and four underscore bucket tags. The production hotkey-route validator accepted the isolated chord. The profile has an explicit `notes` allowlist, empty custom actions, disabled screenshot side effects, and isolated notes/template paths.'

    if ($FixturesOnly) {
        $scenarioStatus = 'BLOCKED'
        $scenarioReason = "$Scenario app launch was not requested in fixture-only mode."
        $runStatus = 'PASS'
        $phase = 'complete'
        Update-Scenarios
        Write-Event 'fixture_only_complete' @{ fixture_manifest_sha256 = $fixtureHash }
    } elseif (!$interactive -or $sessionId -eq 0) {
        $scenarioStatus = 'NOT_RUN_ENVIRONMENT'
        $scenarioReason = 'Windows reports no interactive user session; no app was launched and no screenshot was fabricated.'
        $runStatus = 'NOT_RUN_ENVIRONMENT'
        $phase = 'complete'
        Update-Scenarios
        Write-Event 'gui_not_run_environment' @{ user_interactive = $interactive; session_id = $sessionId }
        Write-Summary $runStatus 'Fixture typed-load checks passed. GUI launch was skipped because this process is not in an interactive Windows user session; no screenshot was fabricated.'
    } else {
        $phase = 'app_build'
        Write-Event 'app_build_started' @{ target_directory = $targetDirectory }
        $buildResult = Invoke-BoundedCommand $cargoExecutable @('build', '--manifest-path', (Join-Path $repoRoot 'Cargo.toml'), '--profile', 'fast-dev', '--features', $cargoFeatures, '--bin', 'multi_launcher') $repoRoot $buildStdoutPath $buildStderrPath 'qa-framebuffer-capture app build'
        $buildExitCode = $buildResult.ExitCode
        Write-Event 'app_build_completed' @{ exit_code = $buildExitCode }
        if ($buildExitCode -ne 0) { throw 'qa-framebuffer-capture app build failed; see build logs' }

        $executablePath = Join-Path $targetDirectory 'fast-dev\multi_launcher.exe'
        if (!(Test-Path -LiteralPath $executablePath -PathType Leaf)) { throw "built app executable is missing: $executablePath" }
        $executablePath = [IO.Path]::GetFullPath($executablePath)
        $executableHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $executablePath).Hash.ToLowerInvariant()
        $buildReceiptIdentity = "cargo-fast-dev;features=$cargoFeatures;head=$headSha;exe_sha256=$executableHash"

        if (Test-Path -LiteralPath $captureRoot) { throw 'QA framebuffer output leaf must be absent before app launch' }
        $qaRequestPath = Join-Path $evidenceRoot 'qa-framebuffer-request.json'
        $isPhase1Run = $Scenario -in @('Phase1Enabled', 'Phase1Disabled')
        $qaRequest = [ordered]@{
            run_id = $runId
            request_id = switch ($Scenario) {
                'QueryProbe' { 'query-only-probe' }
                'Phase1Enabled' { "phase1-enabled-$($Screen.ToLowerInvariant())-catalog" }
                'Phase1Disabled' { 'phase1-disabled-regression' }
                default { 'baseline-dashboard' }
            }
            target = 'ROOT'
            trigger = if ($isPhase1Run) { 'f11' } else { 'startup' }
            capture_count = if ($isPhase1Run) { $phase1CaptureCount } else { 1 }
            diagnostics = $false
            output_dir = $captureRoot
            fixture_manifest_sha256 = $fixtureHash
            build_receipt_identity = $buildReceiptIdentity
        }
        if ($Scenario -eq 'QueryProbe') {
            $qaRequest.jump_mode_observation = [ordered]@{ wait_for_active_hints = $true }
        } elseif ($isPhase1Run) {
            $qaRequest.jump_mode_observation = [ordered]@{ wait_for_active_hints = $false }
        }
        Write-Json $qaRequestPath $qaRequest
        $psi = [Diagnostics.ProcessStartInfo]::new()
        $psi.FileName = $executablePath
        $psi.WorkingDirectory = $profileRoot
        $psi.UseShellExecute = $false
        $psi.CreateNoWindow = $false
        $psi.WindowStyle = [Diagnostics.ProcessWindowStyle]::Normal
        $psi.RedirectStandardOutput = $true
        $psi.RedirectStandardError = $true
        $psi.Environment['ML_NOTES_DIR'] = Join-Path $profileRoot 'notes'
        $psi.Environment['ML_NOTE_TEMPLATES_DIR'] = Join-Path $profileRoot 'templates'
        $psi.Environment['ML_QA_FRAMEBUFFER_REQUEST_MANIFEST'] = $qaRequestPath
        $psi.Environment.Remove('ML_TRACK_C_BENCH_MODE') | Out-Null
        $process = [Diagnostics.Process]::new()
        $process.StartInfo = $psi
        if (!$process.Start()) { throw 'failed to start the isolated Multi Launcher process' }
        $processId = $process.Id
        $processStartUtc = $process.StartTime.ToUniversalTime()
        $processSessionId = $process.SessionId
        $stdoutTask = $process.StandardOutput.ReadToEndAsync()
        $stderrTask = $process.StandardError.ReadToEndAsync()
        Write-Event 'owned_app_started' @{
            pid = $processId
            start_time_utc = $processStartUtc.ToString('O')
            session_id = $processSessionId
            executable = $executablePath
            executable_sha256 = $executableHash
            working_directory = $profileRoot
            ml_notes_dir = $psi.Environment['ML_NOTES_DIR']
            ml_note_templates_dir = $psi.Environment['ML_NOTE_TEMPLATES_DIR']
        }
        $treeObserved = $false
        for ($attempt = 0; $attempt -lt 25; $attempt++) {
            $initialTree = @(Get-OwnedTreeSnapshot)
            if ($initialTree.Count -gt 0) {
                Merge-OwnedTreeSnapshot $initialTree
                $treeObserved = $true
                break
            }
            $process.Refresh()
            if ($process.HasExited) { break }
            Start-Sleep -Milliseconds 100
        }
        if (!$treeObserved) { throw 'could not record the owned process identity in the Windows process table' }
        Write-Event 'owned_process_tree_observed' @{ processes = $ownedTreeSnapshot }
        $phase = 'capture_wait'
        Write-Json $manifestPath ([ordered]@{
            schema_version = 1
            run_id = $runId
            milestone = if ($Scenario -eq 'QueryProbe') { 'M05' } elseif ($isPhase1Run) { 'M08' } else { 'M01' }
            status = $runStatus
            phase = $phase
            started_at_utc = $startedAt.ToString('O')
            repository_root = $repoRoot
            source_head = $headSha
            interactive_session = $interactive
            session_id = $sessionId
            screen_filter = $Screen
            overall_timeout_seconds = $OverallTimeoutSeconds
            environment = [ordered]@{
                os_version = [Environment]::OSVersion.VersionString
                architecture = [Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
                interactive_session = $interactive
                session_id = $processSessionId
                session_name = $env:SESSIONNAME
                window = $appWindowMetadata
            }
            fixture_profile = 'profile'
            fixture_manifest_sha256 = $fixtureHash
            executable = [ordered]@{ path = $executablePath; sha256 = $executableHash; pid = $processId; started_at_utc = $processStartUtc.ToString('O') }
            build_receipt_identity = $buildReceiptIdentity
            capture = [ordered]@{ request = 'qa-framebuffer-request.json'; output_dir = 'capture'; source = if ($isPhase1Run) { 'qa-framebuffer-capture ROOT/F11; CPU UI completion before GPU presentation' } else { 'qa-framebuffer-capture ROOT/Startup; CPU UI completion before GPU presentation' }; count = if ($isPhase1Run) { $phase1CaptureCount } else { 1 }; scenario = $Scenario }
            cleanup = $null
        })
        if ($Scenario -eq 'QueryProbe') {
            Test-ObservedQueryTargetLookup
            Write-Event 'query_target_lookup_regression_passed' @{ target_id = 'fixture-query-target'; hint = 'AA' }
            Initialize-NativeQueryProbe
            Assert-OwnedForeground
            $appWindowMetadata = Get-OwnedWindowMetadata
            Write-Event 'owned_app_window_metadata' @{ os_version = [Environment]::OSVersion.VersionString; architecture = [Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString(); session_id = $processSessionId; window = $appWindowMetadata }
            $phase = 'query_probe_waiting_for_input'
            Write-Summary 'BLOCKED' 'App launched from the verified Jump-enabled isolated profile. Waiting for the actual retained ROOT query state before native input.'
            $baselineObservation = Wait-JumpModeObservation {
                param($state)
                $queryTargets = @($state.targets | Where-Object { $_.is_query_input -and $_.kind -eq 'text_entry' -and $_.eligible })
                $state.root_focused -and $state.root_owner_allowed -and $state.jump_mode_enabled -and
                    !$state.active_session -and $state.query -eq '' -and $state.activation_count -eq 0 -and
                    $queryTargets.Count -eq 1
            } $CaptureTimeoutSeconds 'focused isolated launcher query and disabled Jump Mode baseline'
            Write-Event 'query_baseline_observed' @{ logic_frame_id = $baselineObservation.logic_frame_id; ui_pass_id = $baselineObservation.ui_pass_id; query = $baselineObservation.query; activation_count = $baselineObservation.activation_count }

            Invoke-NativeKeyBatch 'shift-space-leader' ([uint16[]]@(0x10, 0x20)) ([string[]]@('Shift', 'Space'))
            $activeObservation = Wait-JumpModeObservation {
                param($state)
                $queryTargets = @($state.targets | Where-Object { $_.is_query_input -and $_.kind -eq 'text_entry' -and $_.eligible -and $_.hint -match '^[A-Z]+$' })
                $state.root_focused -and $state.root_owner_allowed -and $state.jump_mode_enabled -and
                    $state.active_session -and $state.query -eq '' -and $state.activation_count -eq 0 -and
                    $queryTargets.Count -eq 1
            } $CaptureTimeoutSeconds 'active query hint with preserved query and zero activations'
            $queryTarget = Get-ObservedQueryTarget $activeObservation
            Write-Event 'query_hint_observed' @{ logic_frame_id = $activeObservation.logic_frame_id; ui_pass_id = $activeObservation.ui_pass_id; query_target_id = $queryTarget.target_id; hint = $queryTarget.hint; prefix = $activeObservation.prefix; activation_count = $activeObservation.activation_count }

            $receiptPath = Join-Path $captureRoot 'receipt.json'
            $sourcePngPath = Join-Path $captureRoot 'framebuffer.png'
            $deadline = Get-RunDeadline $CaptureTimeoutSeconds
            while ([DateTime]::UtcNow -lt $deadline) {
                $process.Refresh()
                if ($process.HasExited) {
                    $processExitCode = $process.ExitCode
                    throw "isolated app exited before the query hint capture became ready (exit $processExitCode)"
                }
                if (Test-Path -LiteralPath $receiptPath -PathType Leaf) {
                    try { $candidate = Get-Content -Raw -LiteralPath $receiptPath | ConvertFrom-Json } catch { $candidate = $null }
                    if ($candidate.status -eq 'failed') { throw "QA framebuffer query capture failed: $($candidate.failure)" }
                    if ($candidate.status -eq 'saved' -and (Test-Path -LiteralPath $sourcePngPath -PathType Leaf)) {
                        $captureReceipt = $candidate
                        break
                    }
                }
                Start-Sleep -Milliseconds 100
            }
            if (!$captureReceipt) { throw "timed out after $CaptureTimeoutSeconds seconds waiting for the active-query ROOT/Startup receipt" }
            if ([int]$captureReceipt.process.pid -ne $processId) { throw 'capture receipt PID does not match the process launched by this runner' }
            if ([string]$captureReceipt.executable.sha256 -ne $executableHash) { throw 'capture receipt executable hash does not match the built binary' }
            if ([string]$captureReceipt.fixture_manifest_sha256 -ne $fixtureHash) { throw 'capture receipt fixture hash does not match the isolated fixture' }
            $capturedObservation = $captureReceipt.captured.jump_mode_observation
            if (!$capturedObservation -or !$capturedObservation.active_session -or !$capturedObservation.root_focused -or !$capturedObservation.root_owner_allowed) { throw 'capture receipt has no matching active, owned ROOT query-hint snapshot' }
            if ($capturedObservation.query -ne '' -or $capturedObservation.activation_count -ne 0) { throw 'capture receipt query changed or an action executed before the query hint was selected' }
            $capturedQueryTarget = Get-ObservedQueryTarget $capturedObservation
            if ($capturedQueryTarget.target_id -ne $queryTarget.target_id -or $capturedQueryTarget.hint -ne $queryTarget.hint) { throw 'captured query hint does not match the latest observed active target map' }
            if ($captureReceipt.captured.logic_frame_id -ne $capturedObservation.logic_frame_id -or $captureReceipt.captured.ui_pass_id -ne $capturedObservation.ui_pass_id) { throw 'capture receipt does not identify the same retained UI pass as its Jump Mode snapshot' }
            $png = Get-PngDimensions $sourcePngPath
            $pngHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $sourcePngPath).Hash.ToLowerInvariant()
            if ([string]$captureReceipt.saved.png_sha256 -ne $pngHash) { throw 'saved PNG hash does not match the native ROOT screenshot receipt' }
            Copy-Item -LiteralPath $sourcePngPath -Destination (Join-Path $screenshotsRoot '00_query_hints.png')
            $screenshotPath = 'screenshots/00_query_hints.png'
            $screenshotWidth = $png.Width
            $screenshotHeight = $png.Height
            Write-Event 'query_hint_capture_saved' @{ screenshot = $screenshotPath; width = $screenshotWidth; height = $screenshotHeight; bytes = $png.Bytes; sha256 = $pngHash; capture_semantics = $captureReceipt.capture_semantics; receipt = 'capture/receipt.json'; query_target_id = $capturedQueryTarget.target_id; hint = $capturedQueryTarget.hint; logic_frame_id = $capturedObservation.logic_frame_id; ui_pass_id = $capturedObservation.ui_pass_id }

            $hintIndex = 0
            foreach ($character in ([string]$queryTarget.hint).ToCharArray()) {
                $hintIndex++
                $hintKey = [uint16][char]$character
                Invoke-NativeKeyBatch "query-target-hint-$hintIndex" ([uint16[]]@($hintKey)) ([string[]]@([string]$character))
            }
            $completedObservation = Wait-JumpModeObservation {
                param($state)
                $state.root_focused -and $state.root_owner_allowed -and !$state.active_session -and
                    $state.query -eq '' -and $state.activation_count -eq 0 -and $state.query_focused -and
                    $state.outcome.kind -eq 'completed' -and $state.outcome.target_id -eq $state.query_target_id -and
                    $state.focused_destination -eq $state.query_target_id
            } $CaptureTimeoutSeconds 'completed query target focus handoff with no action execution'
            Write-Event 'query_hint_focus_completed' @{ logic_frame_id = $completedObservation.logic_frame_id; ui_pass_id = $completedObservation.ui_pass_id; query_target_id = $completedObservation.query_target_id; outcome = $completedObservation.outcome; query = $completedObservation.query; activation_count = $completedObservation.activation_count; focused_widget_id = $completedObservation.focused_widget_id }

            Invoke-NativeKeyBatch 'normal-query-edit-x' ([uint16[]]@([char]'X')) ([string[]]@('X'))
            $editedObservation = Wait-JumpModeObservation {
                param($state)
                $state.root_focused -and $state.root_owner_allowed -and !$state.active_session -and
                    $state.query -eq 'x' -and $state.activation_count -eq 0 -and $state.query_focused
            } $CaptureTimeoutSeconds 'normal query text edit after Jump Mode focus handoff'
            Write-Event 'query_edit_observed' @{ logic_frame_id = $editedObservation.logic_frame_id; ui_pass_id = $editedObservation.ui_pass_id; query = $editedObservation.query; activation_count = $editedObservation.activation_count; focused_widget_id = $editedObservation.focused_widget_id }
            $scenarioStatus = 'PASS'
            $scenarioReason = 'Native Shift+Space displayed the actual query hint without changing the query or executing actions; the map-derived query hint focused the real query TextEntry, and native X then edited the query normally. The retained capture receipt references the matching active-hint UI pass.'
            $runStatus = 'PASS'
            $phase = 'query_probe_complete'
            Update-Scenarios
            Write-Event 'query_probe_assertions_passed' @{ query_target_id = $completedObservation.query_target_id; hint = $queryTarget.hint; query_after_leader = ''; query_after_edit = $editedObservation.query; activation_count = $editedObservation.activation_count; capture = 'screenshots/00_query_hints.png' }
            Write-Summary 'PASS' 'Native Shift+Space, active query hint, map-derived focus handoff, unchanged activation count, normal query edit, and receipt-bound PNG assertions passed. Closing the app and verifying its owned process tree.'
        } elseif ($isPhase1Run) {
            Initialize-NativeQueryProbe
            Assert-OwnedForeground
            $appWindowMetadata = Get-OwnedWindowMetadata
            Write-Event 'owned_app_window_metadata' @{ os_version = [Environment]::OSVersion.VersionString; architecture = [Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString(); session_id = $processSessionId; window = $appWindowMetadata }
            $phase = 'phase1_waiting_for_input'
            Write-Summary 'BLOCKED' "App launched from the verified $fixtureScenarioArgument profile. Waiting for the actual retained ROOT targets before running bounded native Phase 1 input."
            if ($Scenario -eq 'Phase1Enabled') {
                Invoke-Phase1EnabledWorkflow
            } else {
                Invoke-Phase1DisabledWorkflow
            }
        } else {
            Write-Summary 'BLOCKED' 'App launched from the isolated profile. Waiting for a ROOT/Startup framebuffer receipt and a valid PNG.'

            $receiptPath = Join-Path $captureRoot 'receipt.json'
            $sourcePngPath = Join-Path $captureRoot 'framebuffer.png'
            $deadline = Get-RunDeadline $CaptureTimeoutSeconds
            while ([DateTime]::UtcNow -lt $deadline) {
                $process.Refresh()
                if ($process.HasExited) {
                    $processExitCode = $process.ExitCode
                    throw "isolated app exited before the capture became ready (exit $processExitCode)"
                }
                if (Test-Path -LiteralPath $receiptPath -PathType Leaf) {
                    $candidate = Get-Content -Raw -LiteralPath $receiptPath | ConvertFrom-Json
                    if ($candidate.status -eq 'failed') {
                        throw "QA framebuffer capture failed: $($candidate.failure)"
                    }
                    if ($candidate.status -eq 'saved' -and (Test-Path -LiteralPath $sourcePngPath -PathType Leaf)) {
                        $captureReceipt = $candidate
                        break
                    }
                }
                Start-Sleep -Milliseconds 200
            }
            if (!$captureReceipt) { throw "timed out after $CaptureTimeoutSeconds seconds waiting for the ROOT/Startup capture receipt" }
            if ([int]$captureReceipt.process.pid -ne $processId) { throw 'capture receipt PID does not match the process launched by this runner' }
            if ([string]$captureReceipt.executable.sha256 -ne $executableHash) { throw 'capture receipt executable hash does not match the built binary' }
            if ([string]$captureReceipt.fixture_manifest_sha256 -ne $fixtureHash) { throw 'capture receipt fixture hash does not match the isolated fixture' }
            $png = Get-PngDimensions $sourcePngPath
            Copy-Item -LiteralPath $sourcePngPath -Destination (Join-Path $screenshotsRoot '00_baseline_dashboard.png')
            $screenshotPath = 'screenshots/00_baseline_dashboard.png'
            $screenshotWidth = $png.Width
            $screenshotHeight = $png.Height
            $scenarioStatus = 'BLOCKED'
            $scenarioReason = 'A real ROOT/Startup framebuffer and matching receipt were saved. Dashboard fixture content is not confirmed by capture metadata; no OCR or pixel-golden assertion is used.'
            $runStatus = 'BLOCKED'
            Update-Scenarios
            Write-Event 'baseline_capture_saved' @{
                screenshot = $screenshotPath
                width = $screenshotWidth
                height = $screenshotHeight
                bytes = $png.Bytes
                capture_semantics = $captureReceipt.capture_semantics
                receipt = 'capture/receipt.json'
            }
            $phase = 'process_cleanup'
            Write-Summary 'BLOCKED' 'A real app ROOT/Startup framebuffer was saved and validated. Dashboard fixture content remains unconfirmed by capture metadata. Closing the app and verifying its owned process tree.'
        }
    }
    }
} catch {
    $runStatus = 'FAIL'
    if ($fixtureScenarioStatus -eq 'BLOCKED') {
        $fixtureScenarioStatus = 'FAIL'
        $fixtureScenarioReason = $_.Exception.Message
    }
    if ($processId -or (!$FixturesOnly -and $interactive -and $sessionId -ne 0)) {
        $scenarioStatus = 'FAIL'
        $scenarioReason = $_.Exception.Message
    }
    $failureText = $_.Exception.Message
    Write-Event 'run_failed' @{ message = $failureText }
    if (Test-Path -LiteralPath $evidenceRoot) {
        Update-Scenarios
        Write-Summary $runStatus 'The run did not meet its current acceptance criteria.'
    }
} finally {
    Close-OwnedProcess
    if (Test-Path -LiteralPath $evidenceRoot) {
        $cleanup = [ordered]@{
            pid = $processId
            start_time_utc = if ($processStartUtc) { $processStartUtc.ToString('O') } else { $null }
            session_id = $processSessionId
            graceful_close_requested = $gracefulCloseRequested
            forced_tree_cleanup = $forcedTreeCleanup
            exit_code = $processExitCode
            post_cleanup_state = $postCleanupState
            owned_tree_verified = $ownedTreeVerified
            owned_tree_snapshot = $ownedTreeSnapshot
            method = if ($forcedTreeCleanup) { 'retained Process handle Kill(entireProcessTree=true)' } elseif ($processId) { 'CloseMainWindow and bounded wait' } else { 'not launched' }
        }
        Write-Json $lifecyclePath $cleanup
        Write-Event 'lifecycle_finalized' $cleanup
        if ($postCleanupState -in @('root_still_alive', 'owned_tree_still_alive', 'cleanup_uncertain') -or $cleanupIssues.Count -gt 0) {
            $runStatus = 'FAIL'
            if (!$failureText) { $failureText = "owned process cleanup state is $postCleanupState" }
            if ($processId) {
                $scenarioStatus = 'FAIL'
                $scenarioReason = $failureText
                Update-Scenarios
            }
        }
        $finishedAt = [DateTime]::UtcNow
        $finalManifest = [ordered]@{
            schema_version = 1
            run_id = $runId
            milestone = if ($Scenario -eq 'QueryProbe') { 'M05' } elseif ($Scenario -in @('Phase1', 'Phase1Enabled', 'Phase1Disabled')) { 'M08' } else { 'M01' }
            status = $runStatus
            phase = $phase
            started_at_utc = $startedAt.ToString('O')
            finished_at_utc = $finishedAt.ToString('O')
            elapsed_ms = [int64]($finishedAt - $startedAt).TotalMilliseconds
            repository_root = $repoRoot
            source_head = if ($headSha) { $headSha } else { $null }
            interactive_session = $interactive
            session_id = $sessionId
            fixture_profile = 'profile'
            environment = [ordered]@{
                os_version = [Environment]::OSVersion.VersionString
                architecture = [Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
                interactive_session = $interactive
                session_id = $processSessionId
                session_name = $env:SESSIONNAME
                window = $appWindowMetadata
            }
            fixture_manifest_sha256 = $fixtureHash
            executable = if ($executablePath) { [ordered]@{ path = $executablePath; sha256 = $executableHash; pid = $processId; started_at_utc = if ($processStartUtc) { $processStartUtc.ToString('O') } else { $null } } } else { $null }
            build_receipt_identity = $buildReceiptIdentity
            capture = if ($Scenario -eq 'Phase1') { [ordered]@{ request = 'child qa-framebuffer-request.json'; output_dir = 'screenshots'; index = 'phase1-index.json'; source = 'child native ROOT/F11 receipts bound to retained frames'; count = $phase1Captures.Count } } elseif ($captureReceipt) { [ordered]@{ request = 'qa-framebuffer-request.json'; output_dir = 'capture'; receipt = 'capture/receipt.json'; screenshot = $screenshotPath; width = $screenshotWidth; height = $screenshotHeight; semantics = $captureReceipt.capture_semantics } } else { $null }
            screen_filter = $Screen
            overall_timeout_seconds = $OverallTimeoutSeconds
            cleanup = $cleanup
            failure = $failureText
        }
        Write-Json $manifestPath $finalManifest
        if ($runStatus -eq 'PASS') {
            $passDetails = if ($FixturesOnly) {
                "Fixture validation passed. The $Scenario GUI scenario remains blocked because this invocation did not request an app launch."
            } elseif ($Scenario -eq 'QueryProbe') {
                'Fixture validation, receipt-bound active-query capture, native Shift+Space activation, map-derived query focus handoff, unchanged activation count, normal query text edit, and owned-process cleanup passed.'
            } elseif ($Scenario -eq 'Phase1') {
                'Both isolated Phase 1 app runs passed: enabled catalog 01–14 and disabled-gate case 18. All fifteen screenshots are receipt-bound native ROOT/F11 captures, and each child process tree was verified gone.'
            } else {
                'Fixture validation, ROOT capture, and owned-process cleanup passed.'
            }
            Write-Summary $runStatus $passDetails
        } elseif ($runStatus -eq 'NOT_RUN_ENVIRONMENT') {
            Write-Summary $runStatus 'Fixture verification passed. The GUI run was skipped because this process is outside an interactive Windows user session; no screenshot was fabricated.'
        } elseif ($runStatus -eq 'FAIL') {
            Write-Summary $runStatus 'The run did not meet its current acceptance criteria.'
        } elseif ($runStatus -eq 'BLOCKED') {
            $blockedDetails = if ($Scenario -eq 'QueryProbe') {
                'Fixture validation passed, but the native query-only probe did not complete. See the event log, latest Jump Mode observation, capture receipt, and failure detail.'
            } elseif ($Scenario -eq 'Phase1') {
                'One or both Phase 1 child scenarios did not satisfy all native input, retained capture, fixture identity, or owned cleanup assertions. See phase1-index.json and each child evidence directory.'
            } else {
                'Fixture validation and real ROOT capture completed, with owned-process cleanup recorded. Dashboard fixture content remains blocked until a readiness signal can establish that isolated note results are visible.'
            }
            Write-Summary $runStatus $blockedDetails
        }
        $milestoneLabel = if ($Scenario -eq 'QueryProbe') { 'M05' } elseif ($Scenario -in @('Phase1', 'Phase1Enabled', 'Phase1Disabled')) { 'M08' } else { 'M01' }
        Write-Output "$milestoneLabel status: $runStatus"
        Write-Output "Evidence: $evidenceRoot"
        if ($runStatus -eq 'FAIL') { exit 1 }
    }
    if ($null -eq $previousTargetDirectory) { Remove-Item Env:CARGO_TARGET_DIR -ErrorAction SilentlyContinue } else { $env:CARGO_TARGET_DIR = $previousTargetDirectory }
}
