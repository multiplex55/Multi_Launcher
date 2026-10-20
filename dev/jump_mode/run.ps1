#requires -Version 7.0
[CmdletBinding()]
param(
    [switch]$FixturesOnly,
    [ValidateSet('Baseline', 'QueryProbe', 'Phase1', 'Phase1Enabled', 'Phase1Disabled', 'Phase2')][string]$Scenario = 'Baseline',
    [ValidateSet('All', 'Dashboard', 'Grid')][string]$Screen = 'All',
    [ValidateSet('Settings', 'Notes', 'Todo', 'Timer', 'Paste', 'MkMacro')][string]$Surface = 'Settings',
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
    'Phase2' { "m13_phase2_$($Surface.ToLowerInvariant())" }
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
    'Phase2' { "Native Phase 2 $Surface workflow has not started." }
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
$phase2CaptureCount = if ($Scenario -eq 'Phase2' -and $Surface -eq 'Settings') { 3 } elseif ($Scenario -eq 'Phase2') { 2 } else { 0 }
$phase2NextCaptureIndex = 1
$phase2Captures = [Collections.Generic.List[object]]::new()
$phase2OwnerInstance = $null

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

function Save-WorkflowCapture([int]$CatalogNumber, [string]$Name, [scriptblock]$Predicate, [string]$Description, [switch]$Phase2) {
    $eventPrefix = if ($Phase2) { 'phase2' } else { 'phase1' }
    $captureIndex = if ($Phase2) { $phase2NextCaptureIndex } else { $phase1NextCaptureIndex }
    $captureCount = if ($Phase2) { $phase2CaptureCount } else { $phase1CaptureCount }
    if ($captureIndex -gt $captureCount) { throw "$eventPrefix requested more captures than the QA request allows" }
    $readyState = Wait-JumpModeObservation {
        param($state)
        $state.capture_armed -and [int]$state.capture_index -eq $captureIndex -and
            [int]$state.capture_count -eq $captureCount -and (& $Predicate $state)
    } $CaptureTimeoutSeconds "catalog $CatalogNumber ($Description) retained state and capture rearm"
    Write-Event "${eventPrefix}_capture_requesting" @{
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
        effective_fuzzy_weight = $capturedObservation.effective_fuzzy_weight
        result_generation = $capturedObservation.result_generation
        result_count = $capturedObservation.result_count
        selected_index = $capturedObservation.selected_index
        selected_target_id = $capturedObservation.selected_target_id
        activation_count = $capturedObservation.activation_count
        active_session = $capturedObservation.active_session
        prefix = $capturedObservation.prefix
        navigation_owner = $capturedObservation.navigation_owner
        root_owner_allowed = $capturedObservation.root_owner_allowed
        focused_widget_id = $capturedObservation.focused_widget_id
        focused_navigation_destination = $capturedObservation.focused_navigation_destination
        dialog_state = $capturedObservation.dialog_state
        mkmacro_action_editor_open = $capturedObservation.mkmacro_action_editor_open
        pixels_per_point = $captured.pixels_per_point
        native_pixels_per_point = $captured.native_pixels_per_point
        zoom_factor = $captured.zoom_factor
    }
    if ($Phase2) {
        $script:phase2Captures.Add($record)
        $script:phase2NextCaptureIndex = $captureIndex + 1
    } else {
        $script:phase1Captures.Add($record)
        $script:phase1NextCaptureIndex = $captureIndex + 1
    }
    $script:captureReceipt = $receipt
    $script:screenshotPath = $screenshotRelativePath
    $script:screenshotWidth = $png.Width
    $script:screenshotHeight = $png.Height
    Update-Scenarios
    Write-Event "${eventPrefix}_capture_saved" $record
    if ($captureIndex -lt $captureCount) {
        $nextIndex = $captureIndex + 1
        $null = Wait-JumpModeObservation {
            param($state)
            $state.capture_armed -and [int]$state.capture_index -eq $nextIndex -and
                [int]$state.capture_count -eq $captureCount
            } $CaptureTimeoutSeconds "$eventPrefix capture $captureIndex saved and capture $nextIndex rearmed"
    }
    return $record
}

function Save-Phase1Capture([int]$CatalogNumber, [string]$Name, [scriptblock]$Predicate, [string]$Description) {
    return Save-WorkflowCapture $CatalogNumber $Name $Predicate $Description
}

function Save-Phase2Capture([int]$CatalogNumber, [string]$Name, [scriptblock]$Predicate, [string]$Description) {
    return Save-WorkflowCapture $CatalogNumber $Name $Predicate $Description -Phase2
}

function Add-ReceiptBoundScreenshotAlias($Capture, [string]$AliasName) {
    if ($AliasName -notmatch '^[a-z][a-z0-9_-]*\.png$') { throw "invalid screenshot alias name $AliasName" }
    $source = Join-Path $repoRoot "dev\jump_mode\evidence\$runId\$($Capture.screenshot)"
    $destination = Join-Path $screenshotsRoot $AliasName
    $expectedHash = [string]$Capture.screenshot_sha256
    if (!(Test-Path -LiteralPath $source -PathType Leaf)) { throw "receipt-bound screenshot source is missing for alias $AliasName" }
    if ((Get-FileHash -Algorithm SHA256 -LiteralPath $source).Hash.ToLowerInvariant() -cne $expectedHash) { throw "receipt-bound screenshot source hash changed before alias $AliasName" }
    Copy-Item -LiteralPath $source -Destination $destination
    if ((Get-FileHash -Algorithm SHA256 -LiteralPath $destination).Hash.ToLowerInvariant() -cne $expectedHash) { throw "receipt-bound screenshot alias $AliasName changed captured PNG bytes" }
    Write-Event 'receipt_bound_screenshot_aliased' @{ source = $Capture.screenshot; alias = "screenshots/$AliasName"; screenshot_sha256 = $expectedHash; capture_index = $Capture.capture_index; receipt = $Capture.receipt }
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

function Get-Phase2SurfaceContract {
    switch ($Surface) {
        'Settings' { return [pscustomobject]@{ name = 'Settings'; query = 'settings'; action = 'settings:dialog'; args = $null; owner = 'settings'; dialog = 'settings' } }
        'Notes' { return [pscustomobject]@{ name = 'Notes'; query = 'note'; action = 'note:dialog'; args = $null; owner = 'notes'; dialog = 'notes' } }
        'Todo' { return [pscustomobject]@{ name = 'Todo'; query = 'todo'; action = 'todo:dialog'; args = $null; owner = 'todo'; dialog = 'todo' } }
        'Timer' { return [pscustomobject]@{ name = 'Timer'; query = 'timer'; action = 'timer:dialog:timer'; args = $null; owner = 'timer'; dialog = 'timer' } }
        'Paste' { return [pscustomobject]@{ name = 'Paste'; query = 'paste'; action = 'paste:open'; args = $null; owner = 'paste_sequence'; dialog = 'paste_sequence' } }
        'MkMacro' { return [pscustomobject]@{ name = 'MkMacro'; query = 'mkmacro'; action = 'mkmacro:dialog'; args = $null; owner = 'mkmacro'; dialog = 'mkmacro' } }
        default { throw "unsupported Phase 2 surface $Surface" }
    }
}

function Test-Phase2SemanticIdentity($Target, [string]$LocalIdentity) {
    if ($null -eq $Target.semantic_id -or $Target.semantic_id.kind -cne 'named') { return $false }
    $value = [string]$Target.semantic_id.value
    $suffix = "/$($LocalIdentity.Length):$LocalIdentity"
    return $value.EndsWith($suffix, [StringComparison]::Ordinal)
}

function Test-Phase2TargetGeometry($Target) {
    if ($null -eq $Target.rect -or $null -eq $Target.clipped_rect -or
        $Target.rect.Count -ne 4 -or $Target.clipped_rect.Count -ne 4) { return $false }
    $rect = $Target.rect
    $clip = $Target.clipped_rect
    return ([double]$clip[2] -gt [double]$clip[0]) -and
        ([double]$clip[3] -gt [double]$clip[1]) -and
        ([double]$rect[2] -gt [double]$rect[0]) -and
        ([double]$rect[3] -gt [double]$rect[1])
}

function Test-Phase2TargetFullyVisible($Target) {
    if (!(Test-Phase2TargetGeometry $Target)) { return $false }
    $tolerance = 1.0
    for ($edge = 0; $edge -lt 4; $edge++) {
        if ([Math]::Abs([double]$Target.rect[$edge] - [double]$Target.clipped_rect[$edge]) -gt $tolerance) { return $false }
    }
    return $true
}

function Get-Phase2WaitContext {
    return [pscustomobject]@{
        owner_instance = $script:phase2OwnerInstance
        activation_count = $script:phase2ActivationCount
        query = $script:phase2Query
    }
}

function Test-Phase2OwnerSnapshot($Observation, $Contract, [Nullable[long]]$ExpectedInstance = $null) {
    if ($null -eq $Observation.navigation_owner -or
        [string]$Observation.navigation_owner.kind -cne [string]$Contract.owner -or
        [bool]$Observation.root_owner_allowed) { return $false }
    if ($null -eq $Observation.dialog_state -or
        [string]$Observation.dialog_state.surface -cne [string]$Contract.dialog -or
        $null -eq $Observation.navigation_owner.instance -or
        [long]$Observation.navigation_owner.instance -le 0 -or
        [long]$Observation.dialog_state.instance -ne [long]$Observation.navigation_owner.instance) { return $false }
    if ($null -ne $ExpectedInstance -and [long]$Observation.navigation_owner.instance -ne [long]$ExpectedInstance) { return $false }
    return $true
}

function Get-Phase2Target($Observation, $Contract, [string]$Label, [string]$Kind, [string]$LocalIdentity = '', [switch]$RequireHint, [switch]$RequireFullyVisible) {
    if (!(Test-Phase2OwnerSnapshot $Observation $Contract $script:phase2OwnerInstance)) {
        throw "navigation owner is not the current $($Contract.name) instance while resolving '$Label'"
    }
    $targetCandidates = @($Observation.navigation_targets | Where-Object {
        $_.eligible -and $_.label -ceq $Label -and $_.kind -ceq $Kind -and
            (Test-Phase2TargetGeometry $_) -and
            (!$LocalIdentity -or (Test-Phase2SemanticIdentity $_ $LocalIdentity)) -and
            (!$RequireFullyVisible -or (Test-Phase2TargetFullyVisible $_))
    })
    if ($targetCandidates.Count -ne 1) { throw "observed $($targetCandidates.Count) eligible $Kind targets matched $($Contract.name) label '$Label'" }
    if ($RequireHint -and (!$targetCandidates[0].hint -or [string]$targetCandidates[0].hint -notmatch '^[A-Z]+$')) {
        throw "active hint map did not assign a valid hint to $($Contract.name) target '$Label'"
    }
    return $targetCandidates[0]
}

function Test-Phase2HintTargetReady($Observation, $Contract, $Request, [long]$ExpectedInstance) {
    if (!(Test-Phase2OwnerSnapshot $Observation $Contract $ExpectedInstance)) { return $false }
    return @($Observation.navigation_targets | Where-Object {
        $_.eligible -and $_.label -ceq [string]$Request.label -and $_.kind -ceq [string]$Request.kind -and
            (Test-Phase2TargetGeometry $_) -and
            (!$Request.local_identity -or (Test-Phase2SemanticIdentity $_ ([string]$Request.local_identity))) -and
            (!$Request.require_fully_visible -or (Test-Phase2TargetFullyVisible $_))
    }).Count -gt 0
}

function Get-Phase2RootActionTarget($Observation, $Contract, [switch]$RequireHint) {
    if (!$Observation.root_focused -or !$Observation.root_owner_allowed -or
        [string]$Observation.navigation_owner.kind -cne 'root') {
        throw 'production query route did not return to the eligible ROOT owner'
    }
    $targetCandidates = @($Observation.targets | Where-Object {
        $_.surface -ceq 'launcher_results' -and $_.eligible -and
            $null -ne $_.root_action -and
            [string]$_.root_action.action -ceq [string]$Contract.action -and
            $null -eq $_.root_action.args
    })
    if ($targetCandidates.Count -ne 1) { throw "production query '$($Contract.query)' returned $($targetCandidates.Count) exact $($Contract.action) targets" }
    if ($RequireHint -and (!$targetCandidates[0].hint -or [string]$targetCandidates[0].hint -notmatch '^[A-Z]+$')) {
        throw "active root hint map did not assign a valid hint to $($Contract.action)"
    }
    return $targetCandidates[0]
}

function Test-Phase2RootActionReady($Observation, $Contract) {
    try {
        $null = Get-Phase2RootActionTarget $Observation $Contract
        return $true
    } catch {
        return $false
    }
}

function Start-Phase2Hints($Contract) {
    $waitContext = Get-Phase2WaitContext
    $before = Wait-JumpModeObservation {
        param($state)
        if ($Contract.owner -eq 'root') {
            $state.root_focused -and $state.root_owner_allowed -and $state.navigation_owner.kind -ceq 'root'
        } else {
            Test-Phase2OwnerSnapshot $state $Contract $waitContext.owner_instance
        }
    }.GetNewClosure() $CaptureTimeoutSeconds "$($Contract.name) current owner before Shift+Space"
    if ($before.active_session) { throw "an old Jump Mode session is still active before $($Contract.name) hint input" }
    $queryBefore = [string]$before.query
    $activationCountBefore = [long]$before.activation_count
    Invoke-NativeKeyBatch "phase2-$($Contract.name.ToLowerInvariant())-shift-space" ([uint16[]]@(0x10, 0x20)) ([string[]]@('Shift', 'Space'))
    $active = Wait-JumpModeObservation {
        param($state)
        $sameOwner = if ($Contract.owner -eq 'root') {
            $state.root_focused -and $state.root_owner_allowed -and $state.navigation_owner.kind -ceq 'root'
        } else {
            Test-Phase2OwnerSnapshot $state $Contract $waitContext.owner_instance
        }
        $sameOwner -and $state.jump_mode_enabled -and $state.active_session -and
            [string]$state.query -ceq $queryBefore -and [long]$state.activation_count -eq $activationCountBefore
    }.GetNewClosure() $CaptureTimeoutSeconds "$($Contract.name) active hint map without query/action changes"
    Write-Event 'phase2_hint_map_active' @{ surface = $Contract.name; owner = $active.navigation_owner; logic_frame_id = $active.logic_frame_id; ui_pass_id = $active.ui_pass_id; activation_count = $active.activation_count; query = $active.query }
    return $active
}

function Complete-Phase2HintTarget($Contract, [string]$Label, [string]$Kind, [string]$LocalIdentity = '', [switch]$TableFocus, [switch]$RequireFullyVisible) {
    $waitContext = Get-Phase2WaitContext
    $active = Start-Phase2Hints $Contract
    $targetRequest = [pscustomobject]@{
        label = $Label
        kind = $Kind
        local_identity = $LocalIdentity
        require_fully_visible = [bool]$RequireFullyVisible
    }
    $queryBeforeTarget = [string]$active.query
    $activationCountBeforeTarget = [long]$active.activation_count
    $active = Wait-JumpModeObservation {
        param($state)
        $state.active_session -and [string]$state.query -ceq $queryBeforeTarget -and
            [long]$state.activation_count -eq $activationCountBeforeTarget -and
            (Test-Phase2HintTargetReady $state $Contract $targetRequest ([long]$waitContext.owner_instance))
    }.GetNewClosure() $CaptureTimeoutSeconds "$($Contract.name) eligible '$Label' target in the active map"
    $target = Get-Phase2Target $active $Contract $Label $Kind $LocalIdentity -RequireHint -RequireFullyVisible:$RequireFullyVisible
    $targetId = [string]$target.target_id
    $hint = [string]$target.hint
    $queryBefore = [string]$active.query
    $activationCountBefore = [long]$active.activation_count
    Write-Event 'phase2_hint_target_observed' @{ surface = $Contract.name; target_id = $targetId; semantic_id = $target.semantic_id; target_generation = $target.target_generation; label = $target.label; kind = $target.kind; hint = $hint; rect = $target.rect; clipped_rect = $target.clipped_rect }
    Send-ObservedHint "phase2-$($Contract.name.ToLowerInvariant())-hint" $hint
    $completed = Wait-JumpModeObservation {
        param($state)
        $ownerMatches = Test-Phase2OwnerSnapshot $state $Contract $waitContext.owner_instance
        $targetMatches = $null -ne $state.outcome -and $state.outcome.kind -ceq 'completed' -and
            [string]$state.outcome.target_id -ceq $targetId
        $ownerMatches -and !$state.active_session -and $targetMatches -and
            [string]$state.query -ceq $queryBefore -and [long]$state.activation_count -eq $activationCountBefore
    }.GetNewClosure() $CaptureTimeoutSeconds "$($Contract.name) map-derived completion of '$Label' without activation"
    if ($TableFocus) {
        if ($completed.dialog_state.mkmacro_table_focused -ne $true -or
            [string]$completed.focused_widget_id -cne [string]$completed.dialog_state.mkmacro_table_focus_id) {
            throw "Mkmacro Select target '$Label' did not hand native focus to its actual table owner"
        }
    } elseif ([string]$completed.focused_navigation_destination -cne $targetId) {
        throw "completed $($Contract.name) target '$Label' did not own native focus"
    }
    Write-Event 'phase2_hint_completed_without_activation' @{ surface = $Contract.name; target_id = $targetId; semantic_id = $target.semantic_id; outcome = $completed.outcome; focused_widget_id = $completed.focused_widget_id; focused_navigation_destination = $completed.focused_navigation_destination; query = $completed.query; activation_count = $completed.activation_count; dialog_state = $completed.dialog_state }
    return [pscustomobject]@{ observation = $completed; target = $target; hint = $hint }
}

function Close-Phase2Hints($Contract) {
    $waitContext = Get-Phase2WaitContext
    $before = Wait-JumpModeObservation {
        param($state)
        (Test-Phase2OwnerSnapshot $state $Contract $waitContext.owner_instance) -and $state.active_session
    }.GetNewClosure() $CaptureTimeoutSeconds "$($Contract.name) active map before safe cancellation"
    $activationCount = [long]$before.activation_count
    $query = [string]$before.query
    Invoke-NativeKeyBatch "phase2-$($Contract.name.ToLowerInvariant())-cancel-hints" ([uint16[]]@(0x1B)) ([string[]]@('Escape'))
    $cancelled = Wait-JumpModeObservation {
        param($state)
        (Test-Phase2OwnerSnapshot $state $Contract $waitContext.owner_instance) -and
            !$state.active_session -and $state.outcome.kind -ceq 'cancelled' -and
            [long]$state.activation_count -eq $activationCount -and [string]$state.query -ceq $query
    }.GetNewClosure() $CaptureTimeoutSeconds "$($Contract.name) hint cancellation without dialog change"
    Write-Event 'phase2_hint_session_cancelled' @{ surface = $Contract.name; logic_frame_id = $cancelled.logic_frame_id; ui_pass_id = $cancelled.ui_pass_id; activation_count = $cancelled.activation_count; query = $cancelled.query }
    return $cancelled
}

function Save-Phase2OwnerCapture([int]$CatalogNumber, [string]$Name, $Contract, [scriptblock]$ExtraPredicate, [string]$Description, [switch]$Inactive) {
    $instance = [long]$script:phase2OwnerInstance
    $activationCount = [long]$script:phase2ActivationCount
    $query = [string]$script:phase2Query
    $activeSessionExpected = !$Inactive
    $predicate = {
        param($state)
        (Test-Phase2OwnerSnapshot $state $Contract $instance) -and
            [bool]$state.active_session -eq $activeSessionExpected -and [long]$state.activation_count -eq $activationCount -and
            [string]$state.query -ceq $query -and (& $ExtraPredicate $state)
    }.GetNewClosure()
    return Save-Phase2Capture $CatalogNumber $Name $predicate $Description
}

function Ensure-Phase2SettingsSectionOpen($Contract, [string]$SectionLabel, [string]$SectionSemantic, [string]$ProbeLabel, [string]$ProbeKind, [string]$ProbeSemantic) {
    $waitContext = Get-Phase2WaitContext
    $state = Wait-JumpModeObservation {
        param($candidate)
        Test-Phase2OwnerSnapshot $candidate $Contract $waitContext.owner_instance
    }.GetNewClosure() $CaptureTimeoutSeconds "Settings owner before checking '$SectionLabel' visibility"
    $probe = @($state.navigation_targets | Where-Object {
        $_.eligible -and $_.label -ceq $ProbeLabel -and $_.kind -ceq $ProbeKind -and
            (Test-Phase2SemanticIdentity $_ $ProbeSemantic)
    })
    if ($probe.Count -eq 1) { return $state }
    if ($probe.Count -gt 1) { throw "Settings section '$SectionLabel' exposed an ambiguous '$ProbeLabel' target" }
    $null = Complete-Phase2HintTarget $Contract $SectionLabel 'button' $SectionSemantic
    Invoke-NativeKeyBatch "phase2-settings-open-section-$($SectionSemantic)" ([uint16[]]@(0x0D)) ([string[]]@('Enter'))
    $opened = Wait-JumpModeObservation {
        param($candidate)
        (Test-Phase2OwnerSnapshot $candidate $Contract $waitContext.owner_instance) -and
            @($candidate.navigation_targets | Where-Object {
                $_.eligible -and $_.label -ceq $ProbeLabel -and $_.kind -ceq $ProbeKind -and
                    (Test-Phase2SemanticIdentity $_ $ProbeSemantic)
            }).Count -eq 1
    }.GetNewClosure() $CaptureTimeoutSeconds "Settings section '$SectionLabel' expanded to show '$ProbeLabel'"
    Write-Event 'phase2_settings_section_opened' @{ section = $SectionLabel; probe_label = $ProbeLabel; target_generation = $opened.dialog_state.target_generation; logic_frame_id = $opened.logic_frame_id }
    return $opened
}

function Invoke-Phase2RootOpener($Contract) {
    $baseline = Ensure-Phase1QueryFocus "phase2-$($Contract.name.ToLowerInvariant())-query"
    if ([string]$baseline.query -cne '') { throw 'fresh Phase 2 ROOT query was not empty before the opener query' }
    $activationCount = [long]$baseline.activation_count
    Send-NativeText "phase2-$($Contract.name.ToLowerInvariant())-opener-query" ([string]$Contract.query)
    $visible = Wait-JumpModeObservation {
        param($state)
        $state.root_focused -and $state.root_owner_allowed -and $state.navigation_owner.kind -ceq 'root' -and
            [string]$state.query -ceq [string]$Contract.query -and [long]$state.activation_count -eq $activationCount -and
            (Test-Phase2RootActionReady $state $Contract)
    }.GetNewClosure() $CaptureTimeoutSeconds "$($Contract.name) production opener query"
    $null = Get-Phase2RootActionTarget $visible $Contract
    $active = Start-Phase2Hints ([pscustomobject]@{ name = 'ROOT'; owner = 'root' })
    $target = Get-Phase2RootActionTarget $active $Contract -RequireHint
    $targetId = [string]$target.target_id
    $hint = [string]$target.hint
    Send-ObservedHint "phase2-$($Contract.name.ToLowerInvariant())-opener-hint" $hint
    $selected = Wait-JumpModeObservation {
        param($state)
        $null -ne $state.outcome -and $state.outcome.kind -ceq 'completed' -and
            [string]$state.outcome.target_id -ceq $targetId -and !$state.active_session -and
            [string]$state.query -ceq [string]$Contract.query -and
            [long]$state.activation_count -eq $activationCount -and
            [string]$state.selected_target_id -ceq $targetId -and $state.query_focused -and
            [string]$state.selected_action.action -ceq [string]$Contract.action -and
            $null -eq $state.selected_action.args
    }.GetNewClosure() $CaptureTimeoutSeconds "$($Contract.name) exact production opener selected from the observed hint map"
    Write-Event 'phase2_root_opener_selected' @{ surface = $Contract.name; target_id = $targetId; semantic_id = $target.semantic_id; target_generation = $target.target_generation; hint = $hint; action = $selected.selected_action.action; args = $selected.selected_action.args; query = $selected.query; activation_count = $selected.activation_count }
    Invoke-NativeKeyBatch "phase2-$($Contract.name.ToLowerInvariant())-fresh-enter-opener" ([uint16[]]@(0x0D)) ([string[]]@('Enter'))
    $opened = Wait-JumpModeObservation {
        param($state)
        (Test-Phase2OwnerSnapshot $state $Contract) -and
            [long]$state.activation_count -eq ($activationCount + 1) -and
            [string]$state.last_action.action_id -ceq [string]$Contract.action -and
            $null -eq $state.last_action.args -and [string]$state.last_action.source -ceq 'enter' -and
            [string]$state.query -ceq [string]$Contract.query
    }.GetNewClosure() $CaptureTimeoutSeconds "$($Contract.name) owner after one fresh Enter production action"
    if ([long]$opened.navigation_owner.instance -le 0) { throw "$($Contract.name) owner has no nonzero instance identity" }
    $script:phase2OwnerInstance = [long]$opened.navigation_owner.instance
    $script:phase2ActivationCount = [long]$opened.activation_count
    $script:phase2Query = [string]$opened.query
    Write-Event 'phase2_root_opener_activated_once' @{ surface = $Contract.name; owner = $opened.navigation_owner; dialog_state = $opened.dialog_state; action = $opened.last_action.action_id; args = $opened.last_action.args; source = $opened.last_action.source; activation_count = $opened.activation_count; query = $opened.query; logic_frame_id = $opened.logic_frame_id; ui_pass_id = $opened.ui_pass_id }
    return $opened
}

function Test-Phase2WorkflowHelpers {
    $contract = [pscustomobject]@{ name = 'Todo'; owner = 'todo'; dialog = 'todo' }
    $target = [pscustomobject]@{
        target_id = 'opaque-observer-id'
        label = 'New Todo'
        kind = 'text_entry'
        eligible = $true
        semantic_id = [pscustomobject]@{ kind = 'named'; value = 'todo/8:new-todo' }
        hint = 'AB'
        rect = @(1, 2, 30, 40)
        clipped_rect = @(2, 3, 29, 39)
    }
    if (!(Test-Phase2SemanticIdentity $target 'new-todo') -or !(Test-Phase2TargetGeometry $target)) {
        throw 'Phase 2 helper regression rejected a structured semantic and visible target geometry'
    }
    $observation = [pscustomobject]@{
        navigation_owner = [pscustomobject]@{ kind = 'todo'; instance = 7 }
        root_owner_allowed = $false
        dialog_state = [pscustomobject]@{ surface = 'todo'; instance = 7 }
        navigation_targets = @($target)
    }
    $savedOwnerInstance = $script:phase2OwnerInstance
    $savedActivationCount = $script:phase2ActivationCount
    $savedQuery = $script:phase2Query
    $script:phase2OwnerInstance = [long]7
    $script:phase2ActivationCount = [long]11
    $script:phase2Query = 'phase2 closure scope probe'
    $resolved = Get-Phase2Target $observation $contract 'New Todo' 'text_entry' 'new-todo' -RequireHint
    if ($resolved.target_id -cne 'opaque-observer-id' -or $resolved.hint -cne 'AB') {
        throw 'Phase 2 target lookup did not return the exact opaque ID and observed hint'
    }
    if (!(Test-Phase2HintTargetReady $observation $contract ([pscustomobject]@{ label = 'New Todo'; kind = 'text_entry'; local_identity = 'new-todo'; require_fully_visible = $false }) 7)) {
        throw 'Phase 2 target readiness did not recognize the exact eligible named control'
    }
    $pendingTarget = $observation.PSObject.Copy()
    $pendingTarget.navigation_targets = @()
    if (Test-Phase2HintTargetReady $pendingTarget $contract ([pscustomobject]@{ label = 'New Todo'; kind = 'text_entry'; local_identity = 'new-todo'; require_fully_visible = $false }) 7) {
        throw 'Phase 2 target readiness accepted a map before its target was published'
    }
    $ambiguous = $observation.PSObject.Copy()
    $ambiguous.navigation_targets = @($target, $target.PSObject.Copy())
    $ambiguousRejected = $false
    try { $null = Get-Phase2Target $ambiguous $contract 'New Todo' 'text_entry' 'new-todo' } catch { $ambiguousRejected = $true }
    if (!$ambiguousRejected) { throw 'Phase 2 target lookup accepted an ambiguous semantic target' }

    $rootContract = [pscustomobject]@{ name = 'Notes'; action = 'note:open' }
    $rootTarget = [pscustomobject]@{
        surface = 'launcher_results'
        eligible = $true
        root_action = [pscustomobject]@{ action = 'note:open'; args = $null }
    }
    $rootObservation = [pscustomobject]@{
        root_focused = $true
        root_owner_allowed = $true
        navigation_owner = [pscustomobject]@{ kind = 'root' }
        targets = @($rootTarget)
    }
    if (!(Test-Phase2RootActionReady $rootObservation $rootContract)) {
        throw 'Phase 2 root readiness did not recognize its exact production action target'
    }
    $rootObservation.targets = @($rootTarget, $rootTarget.PSObject.Copy())
    if (Test-Phase2RootActionReady $rootObservation $rootContract) {
        throw 'Phase 2 root readiness accepted ambiguous production action targets'
    }
    if (!(Test-Phase2EmptyPasteContext ([pscustomobject]@{ paste_context_present = $true; paste_context_has_clipboard_text = $false })) -or
        (Test-Phase2EmptyPasteContext ([pscustomobject]@{ paste_context_present = $false; paste_context_has_clipboard_text = $false })) -or
        (Test-Phase2EmptyPasteContext ([pscustomobject]@{ paste_context_present = $true; paste_context_has_clipboard_text = $true }))) {
        throw 'Phase 2 Paste context predicate must distinguish an explicit empty context from no context or clipboard text'
    }
    if (!(Test-Phase2NavigationFocus ([pscustomobject]@{ focused_navigation_destination = 'opaque-notes-editor-target' }) 'opaque-notes-editor-target') -or
        (Test-Phase2NavigationFocus ([pscustomobject]@{ focused_navigation_destination = '' }) '') -or
        (Test-Phase2NavigationFocus ([pscustomobject]@{ focused_navigation_destination = 'different-target' }) 'opaque-notes-editor-target')) {
        throw 'Phase 2 Notes focus predicate must require a nonempty exact native target identity'
    }
    $waitContext = Get-Phase2WaitContext
    $closurePredicate = {
        param($candidate)
        [long]$candidate.navigation_owner.instance -eq [long]$waitContext.owner_instance -and
            [long]$candidate.activation_count -eq [long]$waitContext.activation_count -and
            [string]$candidate.query -ceq [string]$waitContext.query
    }.GetNewClosure()
    $closureObservation = [pscustomobject]@{
        navigation_owner = [pscustomobject]@{ instance = 7 }
        activation_count = 11
        query = 'phase2 closure scope probe'
    }
    if (!(& $closurePredicate $closureObservation)) {
        throw 'Phase 2 wait-closure regression did not preserve nonzero owner/count/query facts from the calling script scope'
    }
    $wrongClosureObservation = $closureObservation.PSObject.Copy()
    $wrongClosureObservation.navigation_owner = [pscustomobject]@{ instance = 8 }
    if (& $closurePredicate $wrongClosureObservation) {
        throw 'Phase 2 wait-closure regression accepted a mismatched dialog instance'
    }
    $script:phase2OwnerInstance = $savedOwnerInstance
    $script:phase2ActivationCount = $savedActivationCount
    $script:phase2Query = $savedQuery
    Write-Event 'phase2_helper_regressions_passed' @{ semantic_identity = 'structured named target'; exact_opaque_id = $resolved.target_id; observed_hint = $resolved.hint; ambiguous_target_rejected = $ambiguousRejected; exact_root_action_required = $true; ambiguous_root_action_rejected = $true; explicit_empty_paste_context = $true; clipboard_text_absent = $true; exact_nonempty_notes_focus_required = $true; scoped_wait_closure_preserved_nonzero_owner_count_query = $true }
}

function Test-Phase2EmptyPasteContext($DialogState) {
    return $DialogState.paste_context_present -eq $true -and $DialogState.paste_context_has_clipboard_text -eq $false
}

function Test-Phase2NavigationFocus($Observation, [string]$TargetId) {
    return ![string]::IsNullOrWhiteSpace($TargetId) -and
        [string]$Observation.focused_navigation_destination -ceq $TargetId
}

function Invoke-Phase2SettingsWorkflow($Contract) {
    $opened = Invoke-Phase2RootOpener $Contract
    $waitContext = Get-Phase2WaitContext
    $runtimeFuzzyWeightBefore = [double]$opened.effective_fuzzy_weight
    Start-Phase2Hints $Contract | Out-Null
    $null = Save-Phase2OwnerCapture 15 'settings_hints' $Contract { param($state) $state.dialog_state.settings_draft_jump_mode -eq $true -and $state.dialog_state.settings_committed_jump_mode -eq $true } 'Settings initial active hints and committed Jump Mode state'
    Close-Phase2Hints $Contract | Out-Null

    $null = Complete-Phase2HintTarget $Contract 'Collapse all' 'button' 'sections.collapse_all'
    Invoke-NativeKeyBatch 'phase2-settings-collapse-sections' ([uint16[]]@(0x0D)) ([string[]]@('Enter'))
    $null = Wait-JumpModeObservation {
        param($state)
        (Test-Phase2OwnerSnapshot $state $Contract $waitContext.owner_instance) -and
            @($state.navigation_targets | Where-Object { $_.label -ceq 'Experimental Jump Mode' }).Count -eq 0
    }.GetNewClosure() $CaptureTimeoutSeconds 'Settings Collapse all removed the formerly expanded checkbox target'
    Ensure-Phase2SettingsSectionOpen $Contract 'Launcher Window & Appearance' 'settings_section_launcher_window' 'Experimental Jump Mode' 'checkbox' 'launcher.experimental_jump_mode' | Out-Null
    $null = Complete-Phase2HintTarget $Contract 'Experimental Jump Mode' 'checkbox' 'launcher.experimental_jump_mode'
    Invoke-NativeKeyBatch 'phase2-settings-space-checkbox' ([uint16[]]@(0x20)) ([string[]]@('Space'))
    $checkbox = Wait-JumpModeObservation {
        param($state)
        (Test-Phase2OwnerSnapshot $state $Contract $waitContext.owner_instance) -and
            $state.dialog_state.settings_draft_jump_mode -eq $false -and
            $state.dialog_state.settings_committed_jump_mode -eq $true
    }.GetNewClosure() $CaptureTimeoutSeconds 'Settings Jump Mode checkbox draft toggle after Space'

    $null = Complete-Phase2HintTarget $Contract 'Launcher Window & Appearance' 'button' 'settings_section_launcher_window'
    Invoke-NativeKeyBatch 'phase2-settings-collapse-launcher-section' ([uint16[]]@(0x0D)) ([string[]]@('Enter'))
    $null = Wait-JumpModeObservation {
        param($state)
        (Test-Phase2OwnerSnapshot $state $Contract $waitContext.owner_instance) -and
            @($state.navigation_targets | Where-Object { $_.eligible -and $_.kind -ceq 'checkbox' -and (Test-Phase2SemanticIdentity $_ 'launcher.experimental_jump_mode') }).Count -eq 0
    }.GetNewClosure() $CaptureTimeoutSeconds 'Launcher Window section collapsed after checkbox editing'
    Ensure-Phase2SettingsSectionOpen $Contract 'Search & Results' 'settings_section_search_results' 'Fuzzy weight' 'slider' 'search.fuzzy_weight' | Out-Null
    $sliderFocus = Complete-Phase2HintTarget $Contract 'Fuzzy weight' 'slider' 'search.fuzzy_weight' -RequireFullyVisible
    $weightBefore = [double]$sliderFocus.observation.dialog_state.settings_draft_fuzzy_weight
    Invoke-NativeKeyBatch 'phase2-settings-slider-arrow-right' ([uint16[]]@(0x27)) ([string[]]@('Right'))
    $slider = Wait-JumpModeObservation {
        param($state)
        (Test-Phase2OwnerSnapshot $state $Contract $waitContext.owner_instance) -and
            [double]$state.dialog_state.settings_draft_fuzzy_weight -gt $weightBefore -and
            [double]$state.dialog_state.settings_committed_fuzzy_weight -eq [double]$sliderFocus.observation.dialog_state.settings_committed_fuzzy_weight
    }.GetNewClosure() $CaptureTimeoutSeconds 'Settings fuzzy weight actual value change after native Right'
    if ($slider.dialog_state.settings_draft_fuzzy_weight -gt 5.0) { throw 'Settings Fuzzy weight exceeded its configured upper bound' }
    $sliderWeight = [double]$slider.dialog_state.settings_draft_fuzzy_weight
    $numericFocus = Complete-Phase2HintTarget $Contract 'Fuzzy weight value' 'text_entry' 'search.fuzzy_weight/value' -RequireFullyVisible
    Invoke-NativeKeyBatch 'phase2-settings-numeric-arrow-up' ([uint16[]]@(0x26)) ([string[]]@('Up'))
    $numeric = Wait-JumpModeObservation {
        param($state)
        (Test-Phase2OwnerSnapshot $state $Contract $waitContext.owner_instance) -and
            [double]$state.dialog_state.settings_draft_fuzzy_weight -gt $sliderWeight -and
            [double]$state.dialog_state.settings_committed_fuzzy_weight -eq [double]$numericFocus.observation.dialog_state.settings_committed_fuzzy_weight
    }.GetNewClosure() $CaptureTimeoutSeconds 'Settings Fuzzy weight numeric DragValue actual value change after native Up'
    if ($numeric.dialog_state.settings_draft_fuzzy_weight -gt 5.0) { throw 'Settings Fuzzy weight numeric editor exceeded its configured upper bound' }
    Start-Phase2Hints $Contract | Out-Null
    $null = Save-Phase2OwnerCapture 16 'dialog_hints' $Contract {
        param($state)
        $state.dialog_state.settings_draft_jump_mode -eq $false -and
            [double]$state.dialog_state.settings_draft_fuzzy_weight -gt $weightBefore -and
            [double]$state.dialog_state.settings_committed_fuzzy_weight -eq [double]$numericFocus.observation.dialog_state.settings_committed_fuzzy_weight
    }.GetNewClosure() 'Settings checkbox and slider state with the actual dialog hint map'
    Close-Phase2Hints $Contract | Out-Null

    $numericSelection = Complete-Phase2HintTarget $Contract 'Fuzzy weight value' 'text_entry' 'search.fuzzy_weight/value' -RequireFullyVisible
    $numericTargetId = [string]$numericSelection.target.target_id
    $numericBeforeFinalUp = [double]$numericSelection.observation.dialog_state.settings_draft_fuzzy_weight
    $null = Save-Phase2OwnerCapture 17 'dialog_selected' $Contract {
        param($state)
        $state.dialog_state.settings_draft_jump_mode -eq $false -and
            [double]$state.dialog_state.settings_draft_fuzzy_weight -eq $numericBeforeFinalUp -and
            [double]$state.dialog_state.settings_committed_fuzzy_weight -eq [double]$numericFocus.observation.dialog_state.settings_committed_fuzzy_weight -and
            (Test-Phase2NavigationFocus $state $numericTargetId)
    }.GetNewClosure() 'Settings numeric editor with its exact native response focused before the final arrow change' -Inactive
    Invoke-NativeKeyBatch 'phase2-settings-numeric-final-arrow-up' ([uint16[]]@(0x26)) ([string[]]@('Up'))
    $numericFinal = Wait-JumpModeObservation {
        param($state)
        (Test-Phase2OwnerSnapshot $state $Contract $waitContext.owner_instance) -and
            [double]$state.dialog_state.settings_draft_fuzzy_weight -gt $numericBeforeFinalUp -and
            [double]$state.dialog_state.settings_committed_fuzzy_weight -eq [double]$numericFocus.observation.dialog_state.settings_committed_fuzzy_weight
    }.GetNewClosure() $CaptureTimeoutSeconds 'Settings focused numeric editor changes after a fresh native Up'
    if ($numericFinal.dialog_state.settings_draft_fuzzy_weight -gt 5.0) { throw 'Settings Fuzzy weight numeric editor exceeded its configured upper bound' }
    $cancel = Complete-Phase2HintTarget $Contract 'Cancel' 'button' 'footer.cancel'
    Invoke-NativeKeyBatch 'phase2-settings-cancel-draft' ([uint16[]]@(0x0D)) ([string[]]@('Enter'))
    $closed = Wait-JumpModeObservation {
        param($state)
        $state.navigation_owner.kind -ceq 'root' -and $state.root_focused -and
            [long]$state.activation_count -eq [long]$waitContext.activation_count -and
            $state.jump_mode_enabled -eq $true -and !$state.active_session -and
            [double]$state.effective_fuzzy_weight -eq $runtimeFuzzyWeightBefore
    }.GetNewClosure() $CaptureTimeoutSeconds 'Settings Cancel restores committed Jump Mode without saving drafts'
    Write-Event 'phase2_settings_cancel_verified' @{ initial_jump_mode = $true; committed_jump_mode = $closed.jump_mode_enabled; draft_jump_mode_before_cancel = $checkbox.dialog_state.settings_draft_jump_mode; fuzzy_weight_before = $runtimeFuzzyWeightBefore; fuzzy_weight_after_slider = $slider.dialog_state.settings_draft_fuzzy_weight; fuzzy_weight_after_numeric = $numeric.dialog_state.settings_draft_fuzzy_weight; fuzzy_weight_after_final_native_up = $numericFinal.dialog_state.settings_draft_fuzzy_weight; fuzzy_weight_after_cancel = $closed.effective_fuzzy_weight; activation_count = $closed.activation_count; capture_indexes = @($phase2Captures | ForEach-Object { $_.capture_index }) }
    if ($phase2Captures.Count -ne 3) { throw 'Settings workflow did not save all three required receipt-bound screenshots' }
}

function Invoke-Phase2NotesWorkflow($Contract) {
    $opened = Invoke-Phase2RootOpener $Contract
    $waitContext = Get-Phase2WaitContext
    Start-Phase2Hints $Contract | Out-Null
    $null = Save-Phase2OwnerCapture 16 'dialog_hints' $Contract { param($state) $state.dialog_state.notes_open -eq $true -and $state.dialog_state.notes_editing -eq $false } 'Notes list active hints'
    Close-Phase2Hints $Contract | Out-Null
    $null = Complete-Phase2HintTarget $Contract 'Add Note' 'button' 'add-note'
    Invoke-NativeKeyBatch 'phase2-notes-add-draft' ([uint16[]]@(0x0D)) ([string[]]@('Enter'))
    $editing = Wait-JumpModeObservation {
        param($state)
        (Test-Phase2OwnerSnapshot $state $Contract $waitContext.owner_instance) -and $state.dialog_state.notes_editing -eq $true
    }.GetNewClosure() $CaptureTimeoutSeconds 'Notes safe new-note editor after Add Note'
    $entryCount = [int]$editing.dialog_state.notes_entry_count
    $contentFocus = Complete-Phase2HintTarget $Contract 'Note content' 'text_entry' 'edit/content'
    $noteContentTargetId = [string]$contentFocus.target.target_id
    if ([string]::IsNullOrWhiteSpace($noteContentTargetId)) { throw 'Notes content focus did not resolve to a nonempty current target ID' }
    $null = Save-Phase2OwnerCapture 17 'dialog_selected' $Contract {
        param($state)
        $state.dialog_state.notes_editing -eq $true -and
            (Test-Phase2NavigationFocus $state $noteContentTargetId)
    }.GetNewClosure() 'Notes editor with the exact native content target focused' -Inactive
    Send-NativeText 'phase2-notes-safe-draft' 'phase two qa draft'
    $typed = Wait-JumpModeObservation {
        param($state)
        (Test-Phase2OwnerSnapshot $state $Contract $waitContext.owner_instance) -and
            $state.dialog_state.notes_editing -eq $true -and $state.dialog_state.notes_draft_chars -gt 0
    }.GetNewClosure() $CaptureTimeoutSeconds 'Notes isolated draft text entry'
    Invoke-NativeKeyBatch 'phase2-notes-native-enter-newline' ([uint16[]]@(0x0D)) ([string[]]@('Enter'))
    $newline = Wait-JumpModeObservation {
        param($state)
        (Test-Phase2OwnerSnapshot $state $Contract $waitContext.owner_instance) -and
            [int]$state.dialog_state.notes_draft_newlines -gt 0 -and
            [int]$state.dialog_state.notes_entry_count -eq $entryCount -and $state.dialog_state.notes_editing -eq $true
    }.GetNewClosure() $CaptureTimeoutSeconds 'Notes actual multiline newline after native Enter without saving'
    $null = Complete-Phase2HintTarget $Contract 'Cancel' 'button' 'edit/cancel'
    Invoke-NativeKeyBatch 'phase2-notes-cancel-unsaved-draft' ([uint16[]]@(0x0D)) ([string[]]@('Enter'))
    $cancelled = Wait-JumpModeObservation {
        param($state)
        (Test-Phase2OwnerSnapshot $state $Contract $waitContext.owner_instance) -and
            $state.dialog_state.notes_editing -eq $false -and [int]$state.dialog_state.notes_entry_count -eq $entryCount
    }.GetNewClosure() $CaptureTimeoutSeconds 'Notes Cancel discards draft without adding or saving a note'
    Write-Event 'phase2_notes_cancel_verified' @{ original_entry_count = $entryCount; observed_draft_chars = $typed.dialog_state.notes_draft_chars; observed_draft_newlines = $newline.dialog_state.notes_draft_newlines; final_entry_count = $cancelled.dialog_state.notes_entry_count; activation_count = $cancelled.activation_count; opener_action = $opened.last_action.action_id; capture_indexes = @($phase2Captures | ForEach-Object { $_.capture_index }) }
    $namedCapture = $phase2Captures[$phase2Captures.Count - 1]
    Add-ReceiptBoundScreenshotAlias $namedCapture 'notes.png'
}

function Invoke-Phase2TodoWorkflow($Contract) {
    $opened = Invoke-Phase2RootOpener $Contract
    $waitContext = Get-Phase2WaitContext
    Start-Phase2Hints $Contract | Out-Null
    $initial = Wait-JumpModeObservation {
        param($state)
        (Test-Phase2OwnerSnapshot $state $Contract $waitContext.owner_instance) -and
            $state.dialog_state.todo_entry_ids.Count -ge 1 -and $state.dialog_state.todo_pending_clear_confirmation -eq $false
    }.GetNewClosure() $CaptureTimeoutSeconds 'Todo seeded isolated entry list'
    $initialIds = @($initial.dialog_state.todo_entry_ids)
    $null = Save-Phase2OwnerCapture 16 'dialog_hints' $Contract {
        param($state)
        $state.dialog_state.todo_entry_ids.Count -eq $initialIds.Count
    }.GetNewClosure() 'Todo initial isolated list and actual navigation hints'
    Close-Phase2Hints $Contract | Out-Null
    $null = Complete-Phase2HintTarget $Contract 'New Todo' 'text_entry' 'new-todo'
    Send-NativeText 'phase2-todo-safe-add' 'phase two qa item'
    $draft = Wait-JumpModeObservation {
        param($state)
        (Test-Phase2OwnerSnapshot $state $Contract $waitContext.owner_instance) -and
            $state.dialog_state.todo_draft_text_chars -gt 0 -and $state.dialog_state.todo_entry_ids.Count -eq $initialIds.Count
    }.GetNewClosure() $CaptureTimeoutSeconds 'Todo isolated entry field text before fresh Enter'
    Invoke-NativeKeyBatch 'phase2-todo-fresh-enter-safe-add' ([uint16[]]@(0x0D)) ([string[]]@('Enter'))
    $added = Wait-JumpModeObservation {
        param($state)
        (Test-Phase2OwnerSnapshot $state $Contract $waitContext.owner_instance) -and
            $state.dialog_state.todo_entry_ids.Count -eq ($initialIds.Count + 1) -and
            $state.dialog_state.todo_draft_text_chars -eq 0 -and !$state.dialog_state.todo_pending_clear_confirmation
    }.GetNewClosure() $CaptureTimeoutSeconds 'Todo exactly one isolated safe entry added after Enter'
    $newIds = @($added.dialog_state.todo_entry_ids | Where-Object { $_ -notin $initialIds })
    if ($newIds.Count -ne 1) { throw 'Todo entry IDs did not identify exactly one new isolated entry' }
    $null = Save-Phase2OwnerCapture 17 'dialog_selected' $Contract {
        param($state)
        $state.dialog_state.todo_entry_ids.Count -eq ($initialIds.Count + 1) -and $state.dialog_state.todo_entry_ids -contains $newIds[0]
    }.GetNewClosure() 'Todo one safe entry added with current list state' -Inactive
    Write-Event 'phase2_todo_safe_add_verified' @{ fixture_entry_ids = $initialIds; new_entry_id = $newIds[0]; final_entry_count = $added.dialog_state.todo_entry_ids.Count; activation_count = $added.activation_count; opener_action = $opened.last_action.action_id; capture_indexes = @($phase2Captures | ForEach-Object { $_.capture_index }) }
}

function Invoke-Phase2TimerWorkflow($Contract) {
    $opened = Invoke-Phase2RootOpener $Contract
    $waitContext = Get-Phase2WaitContext
    $soundOptionIndex = 1
    $soundOptionLabel = 'Sound: Alarm.wav'
    $soundOptionSemantic = 'sound/option/1'
    Start-Phase2Hints $Contract | Out-Null
    $initial = Wait-JumpModeObservation {
        param($state)
        (Test-Phase2OwnerSnapshot $state $Contract $waitContext.owner_instance) -and
            $state.dialog_state.timer_sound_index -eq 0 -and
            $state.dialog_state.active_timer_count -eq 0 -and $state.dialog_state.active_alarm_count -eq 0
    }.GetNewClosure() $CaptureTimeoutSeconds 'Timer dialog with zero active timers and alarms'
    $null = Save-Phase2OwnerCapture 16 'dialog_hints' $Contract {
        param($state)
        $state.dialog_state.timer_sound_index -eq 0 -and
            $state.dialog_state.active_timer_count -eq 0 -and $state.dialog_state.active_alarm_count -eq 0
    }.GetNewClosure() 'Timer initial empty counts and actual dialog hints'
    Close-Phase2Hints $Contract | Out-Null

    $null = Complete-Phase2HintTarget $Contract 'Sound' 'combo_box' 'sound'
    Invoke-NativeKeyBatch 'phase2-timer-open-sound-combo' ([uint16[]]@(0x0D)) ([string[]]@('Enter'))
    $popupRequest = [pscustomobject]@{
        label = $soundOptionLabel
        kind = 'selectable'
        local_identity = $soundOptionSemantic
        require_fully_visible = $true
    }
    $popup = Wait-JumpModeObservation {
        param($state)
        (Test-Phase2HintTargetReady $state $Contract $popupRequest ([long]$waitContext.owner_instance)) -and
            $state.dialog_state.timer_sound_index -eq 0 -and
            $state.dialog_state.active_timer_count -eq 0 -and $state.dialog_state.active_alarm_count -eq 0
    }.GetNewClosure() $CaptureTimeoutSeconds 'Timer sound popup with its visible production option targets'
    Write-Event 'phase2_timer_sound_popup_ready' @{ target_label = $soundOptionLabel; semantic_id = $soundOptionSemantic; initial_sound_index = $popup.dialog_state.timer_sound_index; activation_count = $popup.activation_count }
    $null = Complete-Phase2HintTarget $Contract $soundOptionLabel 'selectable' $soundOptionSemantic -RequireFullyVisible
    Invoke-NativeKeyBatch 'phase2-timer-select-sound-option' ([uint16[]]@(0x0D)) ([string[]]@('Enter'))
    $soundSelected = Wait-JumpModeObservation {
        param($state)
        (Test-Phase2OwnerSnapshot $state $Contract $waitContext.owner_instance) -and
            $state.dialog_state.timer_sound_index -eq $soundOptionIndex -and
            $state.dialog_state.active_timer_count -eq 0 -and $state.dialog_state.active_alarm_count -eq 0 -and
            [long]$state.activation_count -eq $waitContext.activation_count
    }.GetNewClosure() $CaptureTimeoutSeconds 'Timer sound choice changes through the native option while no timer or alarm starts'
    $remainingPopupOption = @($soundSelected.navigation_targets | Where-Object {
        $_.eligible -and $_.kind -ceq 'selectable' -and (Test-Phase2SemanticIdentity $_ $soundOptionSemantic)
    })
    Write-Event 'phase2_timer_sound_selection_verified' @{ previous_sound_index = $initial.dialog_state.timer_sound_index; selected_sound_index = $soundSelected.dialog_state.timer_sound_index; selected_semantic = $soundOptionSemantic; activation_count = $soundSelected.activation_count; active_timer_count = $soundSelected.dialog_state.active_timer_count; active_alarm_count = $soundSelected.dialog_state.active_alarm_count; active_session = $soundSelected.active_session; remaining_popup_option_count = $remainingPopupOption.Count; remaining_popup_targets = @($soundSelected.navigation_targets | Where-Object { $_.surface_identity.kind -ceq 'popup' } | ForEach-Object { [pscustomobject]@{ label = $_.label; kind = $_.kind; semantic_id = $_.semantic_id; eligible = $_.eligible } }) }

    $durationBodyRequest = [pscustomobject]@{
        label = 'Duration'
        kind = 'text_entry'
        local_identity = 'duration'
        require_fully_visible = $false
    }
    if ($remainingPopupOption.Count -eq 1) {
        Invoke-NativeKeyBatch 'phase2-timer-dismiss-sound-popup' ([uint16[]]@(0x1B)) ([string[]]@('Escape'))
    }
    try {
        $timerBodyReady = Wait-JumpModeObservation {
            param($state)
            $bodyTargetReady = Test-Phase2HintTargetReady $state $Contract $durationBodyRequest ([long]$waitContext.owner_instance)
            $selectedOptionStillRegistered = @($state.navigation_targets | Where-Object {
                $_.eligible -and $_.kind -ceq 'selectable' -and (Test-Phase2SemanticIdentity $_ $soundOptionSemantic)
            }).Count -gt 0
            $bodyTargetReady -and !$state.active_session -and
                $state.dialog_state.timer_sound_index -eq $soundOptionIndex -and
                $state.dialog_state.active_timer_count -eq 0 -and $state.dialog_state.active_alarm_count -eq 0 -and
                (!$remainingPopupOption.Count -or !$selectedOptionStillRegistered)
        }.GetNewClosure() $CaptureTimeoutSeconds 'Timer duration target after the native sound popup has closed'
    } catch {
        $lastObservation = $null
        try { $lastObservation = Read-JumpModeObservation } catch { }
        if ($null -ne $lastObservation) {
            Write-Event 'phase2_timer_popup_dismissal_wait_failed' @{
                error = $_.Exception.Message
                owner = $lastObservation.navigation_owner
                active_session = $lastObservation.active_session
                focused_widget_id = $lastObservation.focused_widget_id
                focused_navigation_destination = $lastObservation.focused_navigation_destination
                dialog_state = $lastObservation.dialog_state
                targets = @($lastObservation.navigation_targets | ForEach-Object {
                    [pscustomobject]@{
                        label = $_.label
                        kind = $_.kind
                        semantic_id = $_.semantic_id
                        eligible = $_.eligible
                        rect = $_.rect
                        clipped_rect = $_.clipped_rect
                        hint = $_.hint
                    }
                })
            }
        }
        throw
    }
    Write-Event 'phase2_timer_sound_popup_closed' @{ dismissal_sent = ($remainingPopupOption.Count -eq 1); duration_target_id = (@($timerBodyReady.navigation_targets | Where-Object { $_.label -ceq 'Duration' -and $_.kind -ceq 'text_entry' -and (Test-Phase2SemanticIdentity $_ 'duration') })[0].target_id); sound_index = $timerBodyReady.dialog_state.timer_sound_index; active_timer_count = $timerBodyReady.dialog_state.active_timer_count; active_alarm_count = $timerBodyReady.dialog_state.active_alarm_count }

    $null = Complete-Phase2HintTarget $Contract 'Duration' 'text_entry' 'duration'
    Send-NativeText 'phase2-timer-safe-duration' '1m'
    $duration = Wait-JumpModeObservation {
        param($state)
        (Test-Phase2OwnerSnapshot $state $Contract $waitContext.owner_instance) -and
            $state.dialog_state.timer_duration_chars -ge 2 -and $state.dialog_state.active_timer_count -eq 0 -and $state.dialog_state.active_alarm_count -eq 0
    }.GetNewClosure() $CaptureTimeoutSeconds 'Timer duration field contains safe text without starting a timer'
    $null = Complete-Phase2HintTarget $Contract 'Name' 'text_entry' 'name'
    Send-NativeText 'phase2-timer-safe-label' 'phase two qa timer'
    $label = Wait-JumpModeObservation {
        param($state)
        (Test-Phase2OwnerSnapshot $state $Contract $waitContext.owner_instance) -and
            $state.dialog_state.timer_label_chars -ge 5 -and $state.dialog_state.active_timer_count -eq 0 -and $state.dialog_state.active_alarm_count -eq 0
    }.GetNewClosure() $CaptureTimeoutSeconds 'Timer name field contains safe text without starting a timer'
    $null = Save-Phase2OwnerCapture 17 'dialog_selected' $Contract {
        param($state)
        $state.dialog_state.timer_duration_chars -ge 2 -and $state.dialog_state.timer_label_chars -ge 5 -and
            $state.dialog_state.timer_sound_index -eq $soundOptionIndex -and
            $state.dialog_state.active_timer_count -eq 0 -and $state.dialog_state.active_alarm_count -eq 0
    }.GetNewClosure() 'Timer safe duration/name drafts and selected sound without Start or Play action' -Inactive
    $null = Complete-Phase2HintTarget $Contract 'Cancel' 'button' 'cancel'
    Invoke-NativeKeyBatch 'phase2-timer-cancel-only' ([uint16[]]@(0x0D)) ([string[]]@('Enter'))
    $closed = Wait-JumpModeObservation {
        param($state)
        $state.navigation_owner.kind -ceq 'root' -and $state.root_focused -and
            [long]$state.activation_count -eq [long]$waitContext.activation_count -and !$state.active_session
    }.GetNewClosure() $CaptureTimeoutSeconds 'Timer Cancel closes without Start, timer or alarm'
    Write-Event 'phase2_timer_cancel_verified' @{ duration_chars = $duration.dialog_state.timer_duration_chars; label_chars = $label.dialog_state.timer_label_chars; selected_sound_index = $soundSelected.dialog_state.timer_sound_index; active_timer_count = $duration.dialog_state.active_timer_count; active_alarm_count = $duration.dialog_state.active_alarm_count; final_activation_count = $closed.activation_count; opener_action = $opened.last_action.action_id; capture_indexes = @($phase2Captures | ForEach-Object { $_.capture_index }) }
}

function Invoke-Phase2PasteWorkflow($Contract, [string]$ProfileRoot) {
    $sequencePath = Join-Path $ProfileRoot 'paste_sequences.json'
    $historyPath = Join-Path $ProfileRoot 'clipboard_history.json'
    $sequenceHashBefore = (Get-FileHash -Algorithm SHA256 -LiteralPath $sequencePath).Hash.ToLowerInvariant()
    $historyHashBefore = (Get-FileHash -Algorithm SHA256 -LiteralPath $historyPath).Hash.ToLowerInvariant()
    $opened = Invoke-Phase2RootOpener $Contract
    $waitContext = Get-Phase2WaitContext
    if (!(Test-Phase2EmptyPasteContext $opened.dialog_state)) {
        throw 'Paste QA opener did not use the explicitly requested empty invocation context'
    }
    Start-Phase2Hints $Contract | Out-Null
    $initial = Wait-JumpModeObservation {
        param($state)
        (Test-Phase2OwnerSnapshot $state $Contract $waitContext.owner_instance) -and
            (Test-Phase2EmptyPasteContext $state.dialog_state) -and
            $state.dialog_state.paste_saved_sequence_ids.Count -eq 1 -and $state.dialog_state.paste_draft_step_count -eq 0
    }.GetNewClosure() $CaptureTimeoutSeconds 'Paste empty context and one inert saved Manual row'
    $sequenceId = [long]$initial.dialog_state.paste_saved_sequence_ids[0]
    $sequenceSemantic = "saved-sequence-$sequenceId"
    $sequenceLabel = 'Jump QA Literal Sequence  ·  1 steps'
    $sequenceTargets = @($initial.navigation_targets | Where-Object {
        $_.eligible -and $_.label -like 'Jump QA Literal Sequence*1 steps' -and $_.kind -ceq 'button' -and
            (Test-Phase2SemanticIdentity $_ $sequenceSemantic)
    })
    if ($sequenceTargets.Count -ne 1 -or !$sequenceTargets[0].hint) { throw 'Paste fixture row does not have one exact observed saved-sequence target and hint' }
    $null = Save-Phase2OwnerCapture 16 'dialog_hints' $Contract {
        param($state)
        (Test-Phase2EmptyPasteContext $state.dialog_state) -and
            $state.dialog_state.paste_saved_sequence_ids.Count -eq 1 -and $state.dialog_state.paste_draft_step_count -eq 0
    }.GetNewClosure() 'Paste sequence list with explicit empty context and actual hint map'
    Close-Phase2Hints $Contract | Out-Null
    $selected = Complete-Phase2HintTarget $Contract $sequenceLabel 'button' $sequenceSemantic
    Invoke-NativeKeyBatch 'phase2-paste-load-saved-row-enter' ([uint16[]]@(0x0D)) ([string[]]@('Enter'))
    $loaded = Wait-JumpModeObservation {
        param($state)
        (Test-Phase2OwnerSnapshot $state $Contract $waitContext.owner_instance) -and
            $state.dialog_state.paste_selected_sequence_id -eq $sequenceId -and
            $state.dialog_state.paste_draft_step_count -eq 1 -and
            (Test-Phase2EmptyPasteContext $state.dialog_state)
    }.GetNewClosure() $CaptureTimeoutSeconds 'Paste saved row loaded into an inert draft by a later Enter'
    if ((Get-FileHash -Algorithm SHA256 -LiteralPath $sequencePath).Hash.ToLowerInvariant() -cne $sequenceHashBefore -or
        (Get-FileHash -Algorithm SHA256 -LiteralPath $historyPath).Hash.ToLowerInvariant() -cne $historyHashBefore) {
        throw 'Paste safe row selection changed the saved sequence or clipboard-history fixture stores'
    }
    $capture = Save-Phase2OwnerCapture 17 'dialog_selected' $Contract {
        param($state)
        $state.dialog_state.paste_selected_sequence_id -eq $sequenceId -and
            $state.dialog_state.paste_draft_step_count -eq 1 -and (Test-Phase2EmptyPasteContext $state.dialog_state)
    }.GetNewClosure() 'Paste selected safe saved sequence with no playback or store changes' -Inactive
    if ((Get-FileHash -Algorithm SHA256 -LiteralPath $sequencePath).Hash.ToLowerInvariant() -cne $sequenceHashBefore -or
        (Get-FileHash -Algorithm SHA256 -LiteralPath $historyPath).Hash.ToLowerInvariant() -cne $historyHashBefore) {
        throw 'Paste capture or selection changed a protected fixture store'
    }
    Write-Event 'phase2_paste_manual_row_loaded_safely' @{ sequence_id = $sequenceId; semantic_target = $sequenceSemantic; completed_target_id = $selected.target.target_id; draft_step_count = $loaded.dialog_state.paste_draft_step_count; invocation_context_present = $loaded.dialog_state.paste_context_present; invocation_context_has_clipboard_text = $loaded.dialog_state.paste_context_has_clipboard_text; sequence_store_sha256_unchanged = $sequenceHashBefore; clipboard_history_sha256_unchanged = $historyHashBefore; activation_count = $loaded.activation_count; opener_action = $opened.last_action.action_id }
    Add-ReceiptBoundScreenshotAlias $capture 'paste.png'
}

function Invoke-Phase2MkMacroWorkflow($Contract) {
    $opened = Invoke-Phase2RootOpener $Contract
    $waitContext = Get-Phase2WaitContext
    Start-Phase2Hints $Contract | Out-Null
    $initial = Wait-JumpModeObservation {
        param($state)
        (Test-Phase2OwnerSnapshot $state $Contract $waitContext.owner_instance) -and
            $state.dialog_state.mkmacro_document_macro_count -eq 1 -and
            $state.dialog_state.mkmacro_active_runtime -eq $false -and $state.dialog_state.mkmacro_recorder_active -eq $false
    }.GetNewClosure() $CaptureTimeoutSeconds 'Mkmacro isolated disabled macro with idle runtime and recorder'
    $revision = [long]$initial.dialog_state.mkmacro_draft_revision
    $null = Save-Phase2OwnerCapture 16 'dialog_hints' $Contract {
        param($state)
        $state.dialog_state.mkmacro_document_macro_count -eq 1 -and
            $state.dialog_state.mkmacro_active_runtime -eq $false -and $state.dialog_state.mkmacro_recorder_active -eq $false
    }.GetNewClosure() 'Mkmacro inert macro list and real action hints'
    Close-Phase2Hints $Contract | Out-Null
    $macro = Complete-Phase2HintTarget $Contract 'Jump QA Inert Macro' 'action' 'macro/1' -TableFocus
    if ($macro.observation.dialog_state.mkmacro_selected_id -ne 1 -or
        $macro.observation.dialog_state.mkmacro_selected_step_ids.Count -ne 0 -or
        [long]$macro.observation.dialog_state.mkmacro_draft_revision -ne $revision) {
        throw 'Mkmacro macro hint changed more than the isolated logical selection/table focus'
    }
    $step = Complete-Phase2HintTarget $Contract 'Step 1 · Delay' 'action' 'step/1/1' -TableFocus
    if ($step.observation.dialog_state.mkmacro_selected_id -ne 1 -or
        $step.observation.dialog_state.mkmacro_selected_step_ids.Count -ne 1 -or
        [long]$step.observation.dialog_state.mkmacro_selected_step_ids[0] -ne 1 -or
        [long]$step.observation.dialog_state.mkmacro_draft_revision -ne $revision -or
        $step.observation.dialog_state.mkmacro_active_runtime -ne $false -or
        $step.observation.dialog_state.mkmacro_recorder_active -ne $false) {
        throw 'Mkmacro step hint did not preserve the inert document/runtime invariants'
    }
    $expectedTableId = [string]$step.observation.dialog_state.mkmacro_table_focus_id
    $capture = Save-Phase2OwnerCapture 17 'dialog_selected' $Contract {
        param($state)
        $state.dialog_state.mkmacro_selected_id -eq 1 -and
            $state.dialog_state.mkmacro_selected_step_ids.Count -eq 1 -and
            [long]$state.dialog_state.mkmacro_selected_step_ids[0] -eq 1 -and
            $state.dialog_state.mkmacro_table_focused -eq $true -and
            [string]$state.focused_widget_id -ceq $expectedTableId -and
            [long]$state.dialog_state.mkmacro_draft_revision -eq $revision -and
            $state.dialog_state.mkmacro_active_runtime -eq $false -and $state.dialog_state.mkmacro_recorder_active -eq $false
    }.GetNewClosure() 'Mkmacro selected disabled Delay step with actual table focus and unchanged document' -Inactive
    Invoke-NativeKeyBatch 'phase2-mkmacro-fresh-enter-safe-editor' ([uint16[]]@(0x0D)) ([string[]]@('Enter'))
    $editor = Wait-JumpModeObservation {
        param($state)
        $state.mkmacro_action_editor_open -eq $true -and
            $state.navigation_owner.kind -ceq 'blocked' -and @($state.navigation_targets).Count -eq 0 -and
            [long]$state.activation_count -eq [long]$waitContext.activation_count -and
            [string]$state.query -ceq [string]$waitContext.query
    }.GetNewClosure() $CaptureTimeoutSeconds 'Mkmacro real action editor blocks background navigation after selected-step Enter'
    if ($editor.dialog_state -ne $null) { throw 'blocked Mkmacro action editor exposed a background dialog snapshot' }
    Write-Event 'phase2_mkmacro_action_editor_opened_safely' @{ macro_id = 1; step_id = 1; editor_open = $editor.mkmacro_action_editor_open; navigation_owner = $editor.navigation_owner; background_target_count = @($editor.navigation_targets).Count; selected_target_id = $step.target.target_id; draft_revision = $revision; runtime_active = $step.observation.dialog_state.mkmacro_active_runtime; recorder_active = $step.observation.dialog_state.mkmacro_recorder_active; activation_count = $editor.activation_count; opener_action = $opened.last_action.action_id }
    Add-ReceiptBoundScreenshotAlias $capture 'mkmacro.png'
}

function Invoke-Phase2Workflow {
    $contract = Get-Phase2SurfaceContract
    Write-Event 'phase2_native_sequence_started' @{ surface = $Contract.name; expected_capture_count = $phase2CaptureCount; query_route = $Contract.query; action = $Contract.action; expected_owner = $Contract.owner; paste_empty_context = ($Surface -eq 'Paste') }
    switch ($Surface) {
        'Settings' { Invoke-Phase2SettingsWorkflow $contract }
        'Notes' { Invoke-Phase2NotesWorkflow $contract }
        'Todo' { Invoke-Phase2TodoWorkflow $contract }
        'Timer' { Invoke-Phase2TimerWorkflow $contract }
        'Paste' { Invoke-Phase2PasteWorkflow $contract $profileRoot }
        'MkMacro' { Invoke-Phase2MkMacroWorkflow $contract }
    }
    if ($phase2Captures.Count -ne $phase2CaptureCount) { throw "Phase 2 $Surface saved $($phase2Captures.Count) of $phase2CaptureCount required captures" }
    $script:scenarioStatus = 'PASS'
    $script:scenarioReason = "Native Phase 2 $Surface query opener used its production action once; exact dialog-owner identity, observed targets and safe later native-key semantics passed with $($phase2Captures.Count) receipt-bound ROOT/F11 captures. No unsafe action, macro playback/recording, timer start, clipboard mutation or private note content was used."
    $script:runStatus = 'PASS'
    $script:phase = 'phase2_complete'
    Update-Scenarios
    Write-Event 'phase2_workflow_assertions_passed' @{ surface = $Surface; capture_count = $phase2Captures.Count; activation_count = $script:phase2ActivationCount; owner_instance = $script:phase2OwnerInstance; screenshots = @($phase2Captures | ForEach-Object { $_.screenshot }); owned_process_cleanup_pending = $true }
    Write-Summary 'PASS' "Native Phase 2 $Surface workflow and its receipt-bound retained states passed. The app will close through its owned window, followed by bounded process-tree cleanup verification."
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
            'Phase2' { 'm13-fixture-validation' }
            default { 'm01-fixture-validation' }
        }
        status = $fixtureScenarioStatus
        expected_surface = if ($Scenario -in @('Phase1', 'Phase1Enabled', 'Phase1Disabled')) { 'typed isolated safe Phase 1 enabled/disabled fixture profiles' } elseif ($Scenario -eq 'Phase2') { "typed isolated Phase 2 $($Surface.ToLowerInvariant()) fixture and required inert stores" } else { 'typed isolated fixture profile with stable safe note data' }
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
            'Phase2' { "m13-phase2-$($Surface.ToLowerInvariant())" }
            default { 'm01-baseline-dashboard' }
        }
        status = $scenarioStatus
        expected_surface = switch ($Scenario) {
            'QueryProbe' { 'active Jump Mode query hint, query focus handoff, and unchanged action count' }
            { $_ -in @('Phase1', 'Phase1Enabled', 'Phase1Disabled') } { "native Phase 1 $Screen dashboard/grid filter, selection, cancellation, note owner, and disabled-gate behavior" }
            'Phase2' { "native production query opener, exact $($Surface.ToLowerInvariant()) owner, and safe observed input behavior" }
            default { 'actual Multi Launcher ROOT viewport with isolated note dashboard' }
        }
        capture_target = 'ROOT'
        capture_trigger = if ($Scenario -in @('Phase1Enabled', 'Phase1Disabled', 'Phase2')) { 'f11' } elseif ($Scenario -eq 'Phase1') { 'child native F11 receipts' } else { 'startup' }
        screenshot = if ($screenshotPath) { $screenshotPath } else { $null }
        pixel_width = $screenshotWidth
        pixel_height = $screenshotHeight
        captures = if ($Scenario -eq 'Phase2') { @($phase2Captures.ToArray()) } else { @($phase1Captures.ToArray()) }
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
        milestone = if ($Scenario -eq 'QueryProbe') { 'M05' } elseif ($Scenario -in @('Phase1', 'Phase1Enabled', 'Phase1Disabled')) { 'M08' } elseif ($Scenario -eq 'Phase2') { 'M13' } else { 'M01' }
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
        'Phase2' { "phase2-$($Surface.ToLowerInvariant())" }
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
    if ($Scenario -eq 'Phase2') { Test-Phase2WorkflowHelpers }
    $fixtureScenarioStatus = 'PASS'
    $fixtureScenarioReason = 'Typed settings/dashboard and production pinned-query config passed; isolated production note loading/search returned all 32 stable note IDs, four underscore bucket tags, and zero actions for the manifest no-results query; the production hotkey-route validator accepted the isolated fixture chord; side-effect safety checks passed.'
    if ($Scenario -eq 'Phase2') { $fixtureScenarioReason += " The typed Phase 2 $($Surface.ToLowerInvariant()) store profile and fixture-bound safety identity passed validation; Next/Cancel remain disabled and error notifications enabled." }
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
        $isPhase2Run = $Scenario -eq 'Phase2'
        $qaRequest = [ordered]@{
            run_id = $runId
            request_id = switch ($Scenario) {
                'QueryProbe' { 'query-only-probe' }
                'Phase1Enabled' { "phase1-enabled-$($Screen.ToLowerInvariant())-catalog" }
                'Phase1Disabled' { 'phase1-disabled-regression' }
                'Phase2' { "phase2-$($Surface.ToLowerInvariant())-native" }
                default { 'baseline-dashboard' }
            }
            target = 'ROOT'
            trigger = if ($isPhase1Run -or $isPhase2Run) { 'f11' } else { 'startup' }
            capture_count = if ($isPhase1Run) { $phase1CaptureCount } elseif ($isPhase2Run) { $phase2CaptureCount } else { 1 }
            diagnostics = $false
            output_dir = $captureRoot
            fixture_manifest_sha256 = $fixtureHash
            build_receipt_identity = $buildReceiptIdentity
        }
        if ($Scenario -eq 'QueryProbe') {
            $qaRequest.jump_mode_observation = [ordered]@{ wait_for_active_hints = $true }
        } elseif ($isPhase1Run) {
            $qaRequest.jump_mode_observation = [ordered]@{ wait_for_active_hints = $false }
        } elseif ($isPhase2Run) {
            $qaRequest.jump_mode_observation = [ordered]@{
                wait_for_active_hints = $false
                phase2 = [ordered]@{
                    surface = $Surface.ToLowerInvariant()
                    use_empty_paste_invocation_context = ($Surface -eq 'Paste')
                }
            }
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
            milestone = if ($Scenario -eq 'QueryProbe') { 'M05' } elseif ($isPhase1Run) { 'M08' } elseif ($isPhase2Run) { 'M13' } else { 'M01' }
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
            capture = [ordered]@{ request = 'qa-framebuffer-request.json'; output_dir = 'capture'; source = if ($isPhase1Run -or $isPhase2Run) { 'qa-framebuffer-capture ROOT/F11; CPU UI completion before GPU presentation' } else { 'qa-framebuffer-capture ROOT/Startup; CPU UI completion before GPU presentation' }; count = if ($isPhase1Run) { $phase1CaptureCount } elseif ($isPhase2Run) { $phase2CaptureCount } else { 1 }; scenario = $Scenario; surface = if ($isPhase2Run) { $Surface } else { $null } }
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
        } elseif ($isPhase2Run) {
            Initialize-NativeQueryProbe
            Assert-OwnedForeground
            $appWindowMetadata = Get-OwnedWindowMetadata
            Write-Event 'owned_app_window_metadata' @{ os_version = [Environment]::OSVersion.VersionString; architecture = [Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString(); session_id = $processSessionId; window = $appWindowMetadata }
            $phase = 'phase2_waiting_for_input'
            Write-Summary 'BLOCKED' "App launched from the verified isolated Phase 2 $Surface profile. Waiting for the production query route and exact active owner before native dialog input."
            Invoke-Phase2Workflow
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
            milestone = if ($Scenario -eq 'QueryProbe') { 'M05' } elseif ($Scenario -in @('Phase1', 'Phase1Enabled', 'Phase1Disabled')) { 'M08' } elseif ($Scenario -eq 'Phase2') { 'M13' } else { 'M01' }
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
            capture = if ($Scenario -eq 'Phase1') {
                [ordered]@{ request = 'child qa-framebuffer-request.json'; output_dir = 'screenshots'; index = 'phase1-index.json'; source = 'child native ROOT/F11 receipts bound to retained frames'; count = $phase1Captures.Count }
            } elseif ($Scenario -eq 'Phase2') {
                [ordered]@{ request = 'qa-framebuffer-request.json'; output_dir = 'capture'; source = 'native ROOT/F11 receipts bound to captured retained observations'; surface = $Surface; count = $phase2Captures.Count; captures = @($phase2Captures.ToArray()) }
            } elseif ($captureReceipt) {
                [ordered]@{ request = 'qa-framebuffer-request.json'; output_dir = 'capture'; receipt = 'capture/receipt.json'; screenshot = $screenshotPath; width = $screenshotWidth; height = $screenshotHeight; semantics = $captureReceipt.capture_semantics }
            } else { $null }
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
            } elseif ($Scenario -eq 'Phase2') {
                "The isolated $Surface production opener and safe native dialog workflow passed. All $($phase2Captures.Count) screenshots are bound to their matching ROOT/F11 captured observations, and owned-process cleanup was verified."
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
            } elseif ($Scenario -eq 'Phase2') {
                "The $Surface native workflow did not satisfy every production-action, dialog-state, receipt-binding, or owned-cleanup assertion. See the event log, per-capture records, and failure detail."
            } else {
                'Fixture validation and real ROOT capture completed, with owned-process cleanup recorded. Dashboard fixture content remains blocked until a readiness signal can establish that isolated note results are visible.'
            }
            Write-Summary $runStatus $blockedDetails
        }
        $milestoneLabel = if ($Scenario -eq 'QueryProbe') { 'M05' } elseif ($Scenario -in @('Phase1', 'Phase1Enabled', 'Phase1Disabled')) { 'M08' } elseif ($Scenario -eq 'Phase2') { 'M13' } else { 'M01' }
        Write-Output "$milestoneLabel status: $runStatus"
        Write-Output "Evidence: $evidenceRoot"
        if ($runStatus -eq 'FAIL') { exit 1 }
    }
    if ($null -eq $previousTargetDirectory) { Remove-Item Env:CARGO_TARGET_DIR -ErrorAction SilentlyContinue } else { $env:CARGO_TARGET_DIR = $previousTargetDirectory }
}
