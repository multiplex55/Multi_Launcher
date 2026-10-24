#requires -Version 7.0
[CmdletBinding()]
param(
    [switch]$FixturesOnly,
    [ValidateSet('Baseline', 'QueryProbe', 'Phase1', 'Phase1Enabled', 'Phase1Disabled', 'Phase2', 'SettingsSearch', 'TargetRepair', 'NoteReplace')][string]$Scenario = 'Baseline',
    [ValidateSet('All', 'Dashboard', 'Grid', 'FinalSmoke')][string]$Screen = 'All',
    [ValidateSet('Settings', 'Notes', 'Todo', 'Timer', 'Paste', 'MkMacro', 'FileSearch')][string]$Surface = 'Settings',
    [ValidateRange(5, 120)][int]$CaptureTimeoutSeconds = 45,
    [ValidateRange(1, 30)][int]$GracefulCloseSeconds = 6,
    [ValidateRange(90, 7200)][int]$OverallTimeoutSeconds = 2400,
    [switch]$NativeOnly,
    [string]$PrebuiltArtifactManifest,
    [string]$NativeBatchDeadlineUtc
)

$ErrorActionPreference = 'Stop'
$repoRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))
$cargoFeatures = 'dev-smoke-binaries,qa-framebuffer-capture'
$isFinalSmoke = $Scenario -eq 'Phase1Enabled' -and $Screen -eq 'FinalSmoke'
$phase2SurfaceKey = switch ($Surface) {
    'FileSearch' { 'file-search' }
    default { $Surface.ToLowerInvariant() }
}
$runMilestone = if ($Scenario -eq 'NoteReplace') { 'G3-M9-B' } elseif ($Scenario -eq 'TargetRepair') { 'G3-M2-B' } elseif ($Scenario -eq 'SettingsSearch') { 'G3-M1-B' } elseif ($isFinalSmoke) { 'M15' } elseif ($Scenario -eq 'QueryProbe') { 'M05' } elseif ($Scenario -in @('Phase1', 'Phase1Enabled', 'Phase1Disabled')) { 'M08' } elseif ($Scenario -eq 'Phase2') { 'M13' } else { 'M01' }
$runStamp = [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssZ')
$runSuffix = [Guid]::NewGuid().ToString('N').Substring(0, 8)
$scenarioTag = if ($isFinalSmoke) { 'm15_final_smoke' } else { switch ($Scenario) {
    'QueryProbe' { 'm05_query_probe' }
    'Phase1' { 'm08_phase1' }
    'Phase1Enabled' { 'm08_phase1_enabled' }
    'Phase1Disabled' { 'm08_phase1_disabled' }
    'Phase2' { "m13_phase2_$phase2SurfaceKey" }
    'SettingsSearch' { 'g3_m1b_settings_search' }
    'TargetRepair' { 'g3_m2b_target_repair' }
    'NoteReplace' { 'g3_m9b_note_replace' }
    default { 'm01_baseline' }
} }
$runId = "jump_${scenarioTag}_${runStamp}_$runSuffix"
if ($Scenario -eq 'Phase1Enabled' -and $Screen -notin @('All', 'FinalSmoke')) { $runId = "${runId}_$($Screen.ToLowerInvariant())" }
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
if (![string]::IsNullOrWhiteSpace($NativeBatchDeadlineUtc)) {
    if ($Scenario -ne 'NoteReplace' -or !$NativeOnly) {
        throw 'NativeBatchDeadlineUtc is supported only by NoteReplace NativeOnly runs'
    }
    $sharedDeadline = [DateTimeOffset]::MinValue
    if (![DateTimeOffset]::TryParse(
        $NativeBatchDeadlineUtc,
        [Globalization.CultureInfo]::InvariantCulture,
        [Globalization.DateTimeStyles]::RoundtripKind,
        [ref]$sharedDeadline
    )) {
        throw 'NativeBatchDeadlineUtc must be an ISO-8601 timestamp with an explicit offset'
    }
    $sharedDeadlineUtc = $sharedDeadline.UtcDateTime
    if ($sharedDeadlineUtc -le $startedAt) { throw 'NativeBatchDeadlineUtc has already expired' }
    if ($sharedDeadlineUtc -lt $runDeadlineUtc) { $runDeadlineUtc = $sharedDeadlineUtc }
} elseif ($Scenario -eq 'NoteReplace' -and $NativeOnly) {
    throw 'NoteReplace NativeOnly requires the shared two-profile NativeBatchDeadlineUtc'
}
$targetDirectory = $null
$previousTargetDirectory = $env:CARGO_TARGET_DIR
$nativeQueryProbeInitialized = $false
$process = $null
$stdoutTask = $null
$stderrTask = $null
$processStartUtc = $null
$processId = $null
$processSessionId = $null
$fixtureHash = $null
$sourceBranch = $null
$sourceDiffSha256 = $null
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
$scenarioReason = if ($isFinalSmoke) { 'M15 final representative smoke has not started.' } else { switch ($Scenario) {
    'QueryProbe' { 'Native Jump Mode query probe has not started.' }
    { $_ -in @('Phase1', 'Phase1Enabled', 'Phase1Disabled') } { 'Native Phase 1 Jump Mode scenarios have not started.' }
    'Phase2' { "Native Phase 2 $Surface workflow has not started." }
    'SettingsSearch' { 'Native Settings search navigation scenario has not started.' }
    'TargetRepair' { 'Native saved-target repair scenario has not started.' }
    'NoteReplace' { 'Native current-note literal replacement scenario has not started.' }
    default { 'Baseline capture has not started.' }
} }
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
    switch ($Screen) { 'Dashboard' { 5 } 'Grid' { 9 } 'FinalSmoke' { 2 } default { 14 } }
} elseif ($Scenario -eq 'Phase1Disabled') { 1 } else { 0 }
$phase1BaseActivationCount = 1
$appWindowMetadata = $null
$finalSmokeResizeMetadata = $null
$phase2CaptureCount = if (($Scenario -eq 'Phase2' -and $Surface -eq 'Settings') -or $Scenario -eq 'SettingsSearch') { 3 } elseif ($Scenario -eq 'Phase2') { 2 } elseif ($Scenario -eq 'TargetRepair') { 1 } else { 0 }
$prebuiltSettingsPath = $null
$prebuiltSettingsSha256 = $null
$phase2NextCaptureIndex = 1
$phase2Captures = [Collections.Generic.List[object]]::new()
$phase2OwnerInstance = $null
$prebuiltSettingsPreInputSha256 = $null
$settingsSearchNavigationSha256 = $null
$settingsSearchNavigationHashVerified = $null
$targetRepairFixtureManifest = $null
$targetRepairExpectedActionPath = $null
$targetRepairExpectedFolderPath = $null
$targetRepairOriginalActionPath = $null
$targetRepairOriginalFolderPath = $null
$targetRepairPrimaryActivationBaseline = $null
$targetRepairActionStorePath = $null
$targetRepairFolderStorePath = $null
$targetRepairFavoriteStorePath = $null
$targetRepairPinStorePath = $null
$targetRepairUsageStorePath = $null
$targetRepairUnrelatedPath = $null
$noteReplaceFixtureScenario = $null
$noteReplaceJumpEnabled = $null
$noteReplaceSourcePath = $null
$noteReplaceOtherPath = $null
$noteReplaceSourceBefore = $null
$noteReplaceOtherBeforeSha256 = $null
$noteReplaceSavedSha256 = $null
$noteReplaceExpectedSourceAfterAll = $null
$noteReplaceExpectedReplacement = $null
$noteReplaceExpectedTitle = $null
$noteReplaceSlug = $null
$noteReplaceBeforeSha256 = $null

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
    $buildSummary = if ($NativeOnly) {
        if ($script:buildReceiptIdentity) {
            "Validated prebuilt artifacts: app=``$script:executablePath`` sha256=$script:executableHash; source=$script:sourceBranch@$script:headSha diff=$script:sourceDiffSha256; build receipt identity=``$script:buildReceiptIdentity``"
        } else {
            'Build: NativeOnly; no Cargo invocation (prebuilt artifact validation pending).'
        }
    } else {
        "Build: ``cargo build --profile fast-dev --features $cargoFeatures --bin multi_launcher``"
    }
    $lines = @(
        "# Jump Mode $Scenario run $runId",
        '',
        "Status: **$Status**",
        '',
        "Started (UTC): $($startedAt.ToString('O'))",
        "Updated (UTC): $([DateTime]::UtcNow.ToString('O'))",
        "Interactive session: $interactive (session $sessionId)",
        "Fixture profile: ``$profileRoot``",
        $buildSummary,
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
    if ($script:nativeQueryProbeInitialized) { return }
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

    [StructLayout(LayoutKind.Sequential)]
    private struct Point {
        public int x;
        public int y;
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
    private static extern bool GetClientRect(IntPtr window, out Rect rect);

    [DllImport("user32.dll", SetLastError = true)]
    private static extern bool ClientToScreen(IntPtr window, ref Point point);

    [DllImport("user32.dll")]
    private static extern IntPtr WindowFromPoint(Point point);

    [DllImport("user32.dll")]
    private static extern IntPtr GetAncestor(IntPtr window, uint flags);

    [DllImport("user32.dll")]
    private static extern bool IsWindowVisible(IntPtr window);

    [DllImport("user32.dll", SetLastError = true)]
    private static extern bool SetCursorPos(int x, int y);

    [DllImport("user32.dll", SetLastError = true)]
    private static extern bool SetWindowPos(IntPtr window, IntPtr insertAfter, int x, int y, int width, int height, uint flags);

    [DllImport("user32.dll", SetLastError = true)]
    private static extern uint GetDpiForWindow(IntPtr window);

    public static uint ForegroundProcessId() {
        IntPtr window = GetForegroundWindow();
        if (window == IntPtr.Zero) return 0;
        uint processId;
        GetWindowThreadProcessId(window, out processId);
        return processId;
    }

    public static uint WindowProcessId(IntPtr window) {
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

    public static IntPtr ForegroundWindowHandle() {
        return GetForegroundWindow();
    }

    public static bool IsVisibleWindow(IntPtr window) {
        return window != IntPtr.Zero && IsWindowVisible(window);
    }

    public static IntPtr RootOwner(IntPtr window) {
        return window == IntPtr.Zero ? IntPtr.Zero : GetAncestor(window, 3u);
    }

    public static IntPtr RootWindow(IntPtr window) {
        return window == IntPtr.Zero ? IntPtr.Zero : GetAncestor(window, 2u);
    }

    public static bool IsOwnedModalForeground(IntPtr rootWindow, uint ownerProcessId) {
        IntPtr foreground = GetForegroundWindow();
        return foreground != IntPtr.Zero && foreground != rootWindow &&
            IsWindowVisible(foreground) && WindowProcessId(foreground) == ownerProcessId &&
            GetAncestor(foreground, 3u) == rootWindow;
    }

    public static bool TryClientToScreen(IntPtr window, int clientX, int clientY, out int screenX, out int screenY) {
        Point point = new Point { x = clientX, y = clientY };
        bool success = ClientToScreen(window, ref point);
        screenX = point.x;
        screenY = point.y;
        return success;
    }

    public static IntPtr WindowAtScreenPoint(int x, int y) {
        return WindowFromPoint(new Point { x = x, y = y });
    }

    public static bool ClickScreenPoint(int x, int y) {
        const uint MOUSEEVENTF_LEFTDOWN = 0x0002;
        const uint MOUSEEVENTF_LEFTUP = 0x0004;
        if (!SetCursorPos(x, y)) return false;
        Input down = new Input();
        down.type = 0;
        down.data.mouse.flags = MOUSEEVENTF_LEFTDOWN;
        Input up = new Input();
        up.type = 0;
        up.data.mouse.flags = MOUSEEVENTF_LEFTUP;
        Input[] inputs = { down, up };
        return SendInput(2, inputs, Marshal.SizeOf(typeof(Input))) == 2;
    }

    public static bool SendUnicodeChar(char character, bool keyUp) {
        const uint KEYEVENTF_KEYUP = 0x0002;
        const uint KEYEVENTF_UNICODE = 0x0004;
        Input input = new Input();
        input.type = 1;
        input.data.keyboard.scanCode = character;
        input.data.keyboard.flags = KEYEVENTF_UNICODE | (keyUp ? KEYEVENTF_KEYUP : 0u);
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

    public static bool TryGetClientBounds(IntPtr window, out int left, out int top, out int right, out int bottom) {
        Rect rect;
        bool success = GetClientRect(window, out rect);
        left = rect.left;
        top = rect.top;
        right = rect.right;
        bottom = rect.bottom;
        return success;
    }

    public static bool ResizeWindowWithoutActivation(IntPtr window, int width, int height) {
        const uint SWP_NOMOVE = 0x0002;
        const uint SWP_NOZORDER = 0x0004;
        const uint SWP_NOACTIVATE = 0x0010;
        return SetWindowPos(window, IntPtr.Zero, 0, 0, width, height,
            SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE);
    }

    public static uint WindowDpi(IntPtr window) {
        return GetDpiForWindow(window);
    }
}
'@
    Add-Type -TypeDefinition $nativeType -Language CSharp -ErrorAction Stop
    $script:nativeQueryProbeInitialized = $true
}

function Get-OwnedWindowMetadata {
    if (!(Test-RetainedProcessIdentity)) { throw 'retained app process identity changed before reading its window metadata' }
    $process.Refresh()
    $window = $process.MainWindowHandle
    if ($window -eq [IntPtr]::Zero) { throw 'owned app main window is unavailable for metadata capture' }
    $windowProcessId = [int][JumpModeNativeProbe]::WindowProcessId($window)
    if ($windowProcessId -ne $processId) { throw "main window belongs to PID $windowProcessId instead of retained PID $processId" }
    $left = 0; $top = 0; $right = 0; $bottom = 0
    $hasBounds = [JumpModeNativeProbe]::TryGetWindowBounds($window, [ref]$left, [ref]$top, [ref]$right, [ref]$bottom)
    $clientLeft = 0; $clientTop = 0; $clientRight = 0; $clientBottom = 0
    $hasClientBounds = [JumpModeNativeProbe]::TryGetClientBounds($window, [ref]$clientLeft, [ref]$clientTop, [ref]$clientRight, [ref]$clientBottom)
    $windowDpi = $null
    try {
        $observedDpi = [JumpModeNativeProbe]::WindowDpi($window)
        if ($observedDpi -gt 0) { $windowDpi = [int]$observedDpi }
    } catch { }
    return [ordered]@{
        hwnd = ('0x{0:X}' -f $window.ToInt64())
        process_id = $windowProcessId
        bounds = if ($hasBounds) { [ordered]@{ left = $left; top = $top; right = $right; bottom = $bottom; width = $right - $left; height = $bottom - $top } } else { $null }
        client_bounds = if ($hasClientBounds) { [ordered]@{ left = $clientLeft; top = $clientTop; right = $clientRight; bottom = $clientBottom; width = $clientRight - $clientLeft; height = $clientBottom - $clientTop } } else { $null }
        dpi = $windowDpi
    }
}

function Resize-OwnedWindowForFinalSmoke {
    if (!$script:isFinalSmoke) { throw 'owned-window resize is only available to the M15 FinalSmoke scenario' }
    if (!(Test-RetainedProcessIdentity)) { throw 'retained app process identity changed before FinalSmoke resize' }
    $process.Refresh()
    $window = $process.MainWindowHandle
    if ($window -eq [IntPtr]::Zero) { throw 'owned app main window is unavailable for FinalSmoke resize' }
    $before = Get-OwnedWindowMetadata
    if ($before.process_id -ne $processId -or $before.hwnd -cne ('0x{0:X}' -f $window.ToInt64())) {
        throw 'FinalSmoke resize HWND does not match the retained app process and main-window handle'
    }
    if (!$before.bounds -or !$before.client_bounds -or $before.bounds.width -le 0 -or $before.bounds.height -le 0 -or
        $before.client_bounds.width -le 0 -or $before.client_bounds.height -le 0) {
        throw 'FinalSmoke resize requires valid owned outer-window and client-area bounds'
    }
    $beforeState = Wait-JumpModeObservation {
        param($state)
        $state.capture_armed -and [int]$state.capture_index -eq 1 -and [int]$state.capture_count -eq 2 -and
            [int]$state.process_pid -eq $processId -and [string]$state.fixture_manifest_sha256 -ceq $fixtureHash -and
            $state.root_focused -and $state.root_owner_allowed
    } ([Math]::Min($CaptureTimeoutSeconds, 15)) 'FinalSmoke initial ROOT observer and two-capture request readiness before resize'

    $targetWidth = [int][Math]::Floor([double]$before.bounds.width * 0.85)
    $targetHeight = [int][Math]::Floor([double]$before.bounds.height * 0.85)
    if ($targetWidth -le 0 -or $targetHeight -le 0 -or $targetWidth -ge $before.bounds.width -or $targetHeight -ge $before.bounds.height) {
        throw 'FinalSmoke resize target is not a strictly smaller positive window size'
    }
    if (!(Test-RetainedProcessIdentity)) { throw 'retained app process identity changed immediately before SetWindowPos' }
    $process.Refresh()
    if ($process.MainWindowHandle -ne $window -or [int][JumpModeNativeProbe]::WindowProcessId($window) -ne $processId) {
        throw 'owned main-window identity changed before SetWindowPos'
    }
    if (![JumpModeNativeProbe]::ResizeWindowWithoutActivation($window, $targetWidth, $targetHeight)) {
        $errorCode = [Runtime.InteropServices.Marshal]::GetLastWin32Error()
        throw "SetWindowPos failed for retained PID $processId / HWND $($before.hwnd) (Win32 error $errorCode)"
    }

    $afterState = Wait-JumpModeObservation {
        param($state)
        $state.capture_armed -and [int]$state.capture_index -eq 1 -and [int]$state.capture_count -eq 2 -and
            [int64]$state.logic_frame_id -gt [int64]$beforeState.logic_frame_id -and
            [int]$state.process_pid -eq $processId -and [string]$state.fixture_manifest_sha256 -ceq $fixtureHash -and
            $state.root_focused -and $state.root_owner_allowed
    } ([Math]::Min($CaptureTimeoutSeconds, 15)) 'new retained ROOT frame after owned-window resize'
    $after = Get-OwnedWindowMetadata
    if ($after.hwnd -cne $before.hwnd -or $after.process_id -ne $processId -or
        !$after.client_bounds -or $after.client_bounds.width -ge $before.client_bounds.width -or
        $after.client_bounds.height -ge $before.client_bounds.height) {
        throw 'FinalSmoke resize did not produce a smaller client area on the same owned HWND'
    }
    if ([JumpModeNativeProbe]::ForegroundProcessId() -ne $processId) {
        throw 'FinalSmoke resize unexpectedly changed foreground ownership; refusing subsequent native input'
    }
    $script:appWindowMetadata = $after
    $script:finalSmokeResizeMetadata = $after
    Write-Event 'final_smoke_owned_window_resized' @{
        hwnd = $after.hwnd
        process_id = $processId
        process_started_at_utc = $processStartUtc.ToString('O')
        requested_outer_size = @{ width = $targetWidth; height = $targetHeight }
        before_window = $before
        after_window = $after
        before_logic_frame_id = $beforeState.logic_frame_id
        after_logic_frame_id = $afterState.logic_frame_id
        capture_index = $afterState.capture_index
        capture_count = $afterState.capture_count
        foreground_process_id = [JumpModeNativeProbe]::ForegroundProcessId()
        activation_flags = @('SWP_NOMOVE', 'SWP_NOZORDER', 'SWP_NOACTIVATE')
    }
    return $after
}

function Get-FinalSmokeCaptureClientBinding($Png, $Captured) {
    if (!$script:isFinalSmoke) { return $null }
    if (!$script:finalSmokeResizeMetadata -or !$script:finalSmokeResizeMetadata.client_bounds) {
        throw 'FinalSmoke capture has no verified post-resize client-area metadata'
    }
    $current = Get-OwnedWindowMetadata
    $expected = $script:finalSmokeResizeMetadata
    if ($current.hwnd -cne $expected.hwnd -or $current.process_id -ne $processId -or
        $current.client_bounds.width -ne $expected.client_bounds.width -or
        $current.client_bounds.height -ne $expected.client_bounds.height) {
        throw 'owned HWND/client dimensions changed between resize readiness and ROOT capture'
    }
    $clientWidth = [int]$current.client_bounds.width
    $clientHeight = [int]$current.client_bounds.height
    $scales = [Collections.Generic.List[object]]::new()
    $scales.Add([pscustomobject]@{ Name = 'client_pixels'; Value = 1.0 })
    if ($null -ne $Captured.native_pixels_per_point -and [double]$Captured.native_pixels_per_point -gt 0) {
        $scales.Add([pscustomobject]@{ Name = 'native_pixels_per_point'; Value = [double]$Captured.native_pixels_per_point })
    }
    if ([double]$Captured.pixels_per_point -gt 0) {
        $scales.Add([pscustomobject]@{ Name = 'pixels_per_point'; Value = [double]$Captured.pixels_per_point })
    }
    $matchedScale = $null
    foreach ($scale in $scales) {
        $expectedWidth = [int][Math]::Round($clientWidth * $scale.Value)
        $expectedHeight = [int][Math]::Round($clientHeight * $scale.Value)
        if ([Math]::Abs([int]$Png.Width - $expectedWidth) -le 1 -and [Math]::Abs([int]$Png.Height - $expectedHeight) -le 1) {
            $matchedScale = $scale.Name
            break
        }
    }
    if (!$matchedScale) {
        throw "ROOT PNG $($Png.Width)x$($Png.Height) does not match owned client area ${clientWidth}x${clientHeight} at client/egui pixel scale"
    }
    return [ordered]@{
        hwnd = $current.hwnd
        process_id = $processId
        client_bounds = $current.client_bounds
        png_width = [int]$Png.Width
        png_height = [int]$Png.Height
        pixels_per_point = [double]$Captured.pixels_per_point
        native_pixels_per_point = $Captured.native_pixels_per_point
        matched_scale = $matchedScale
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

function Wait-SettingsSearchStartupReadiness {
    return Wait-JumpModeObservation {
        param($state)
        if (!(Test-RetainedProcessIdentity)) {
            throw 'retained app PID/birth identity changed while waiting for Settings Search startup readiness'
        }
        $process.Refresh()
        if ($process.HasExited) {
            $script:processExitCode = $process.ExitCode
            throw "isolated app exited before publishing its owned main window and ROOT query response (exit $script:processExitCode)"
        }

        $window = $process.MainWindowHandle
        if ($window -eq [IntPtr]::Zero) { return $false }
        $windowProcessId = [int][JumpModeNativeProbe]::WindowProcessId($window)
        if ($windowProcessId -ne $processId) {
            throw "startup main window belongs to PID $windowProcessId instead of retained PID $processId"
        }

        $queryResponses = @($state.targets | Where-Object {
            $_.surface -ceq 'launcher_results' -and $_.is_query_input -and $_.kind -ceq 'text_entry'
        })
        if ($state.active_session -or $queryResponses.Count -ne 1) {
            return $false
        }
        $queryResponse = $queryResponses[0]
        if ([string]::IsNullOrWhiteSpace([string]$queryResponse.target_id) -or
            $null -eq $state.logic_frame_id -or [long]$state.logic_frame_id -le 0 -or
            $null -eq $state.ui_pass_id -or [long]$state.ui_pass_id -le 0 -or
            !(Test-Phase2TargetGeometry $queryResponse)) {
            return $false
        }

        return $true
    }.GetNewClosure() $CaptureTimeoutSeconds 'retained app main HWND and current ROOT query Response before metadata capture'
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

function Get-OwnedRootHandle {
    if (!(Test-RetainedProcessIdentity)) { throw 'retained app process identity changed before resolving the root HWND' }
    $process.Refresh()
    if ($process.HasExited) {
        $script:processExitCode = $process.ExitCode
        throw "isolated app exited before root HWND use (exit $script:processExitCode)"
    }
    $window = $process.MainWindowHandle
    if ($window -eq [IntPtr]::Zero -or [int][JumpModeNativeProbe]::WindowProcessId($window) -ne $processId) {
        throw 'retained app main HWND is unavailable or belongs to another process'
    }
    return $window
}

function Assert-OwnedRepairRootForeground([IntPtr]$ExpectedRoot) {
    Assert-OwnedForeground
    $currentRoot = Get-OwnedRootHandle
    if ($currentRoot -ne $ExpectedRoot -or
        [JumpModeNativeProbe]::ForegroundWindowHandle() -ne $ExpectedRoot -or
        [JumpModeNativeProbe]::WindowProcessId($ExpectedRoot) -ne $processId) {
        throw 'repair input requires the exact retained app root HWND in the foreground'
    }
    if ([JumpModeNativeProbe]::ModifierIsDown()) {
        throw 'a Ctrl, Alt, Shift, or Windows key is held before repair root input'
    }
}

function Assert-OwnedRepairModalForeground([IntPtr]$ExpectedRoot, [IntPtr]$ExpectedModal) {
    if (!(Test-RetainedProcessIdentity)) { throw 'retained app PID/birth identity changed before native picker input' }
    $process.Refresh()
    if ($process.HasExited) {
        $script:processExitCode = $process.ExitCode
        throw "isolated app exited while its repair picker was open (exit $script:processExitCode)"
    }
    $currentRoot = $process.MainWindowHandle
    $foreground = [JumpModeNativeProbe]::ForegroundWindowHandle()
    if ($currentRoot -ne $ExpectedRoot -or $foreground -ne $ExpectedModal -or
        ![JumpModeNativeProbe]::IsVisibleWindow($ExpectedModal) -or
        ![JumpModeNativeProbe]::IsOwnedModalForeground($ExpectedRoot, [uint32]$processId)) {
        throw 'native picker input refused: foreground is not the exact visible same-process modal owned by the retained launcher HWND'
    }
    if ([JumpModeNativeProbe]::ModifierIsDown()) {
        throw 'a Ctrl, Alt, Shift, or Windows key is held before repair picker input'
    }
}

function Invoke-TargetRepairKeyBatch(
    [string]$Name,
    [uint16[]]$Keys,
    [string[]]$KeyLabels,
    [IntPtr]$ExpectedRoot,
    [IntPtr]$ExpectedForeground
) {
    if ($Keys.Count -ne $KeyLabels.Count -or $Keys.Count -eq 0) { throw 'repair key batch is empty or malformed' }
    if ($ExpectedForeground -eq $ExpectedRoot) {
        Assert-OwnedRepairRootForeground $ExpectedRoot
    } else {
        Assert-OwnedRepairModalForeground $ExpectedRoot $ExpectedForeground
    }
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
    if ([JumpModeNativeProbe]::ModifierIsDown()) { throw "a modifier remained held after $Name" }
    Write-Event 'target_repair_key_batch_sent' @{
        name = $Name
        keys = $KeyLabels
        foreground_hwnd = ('0x{0:X}' -f $ExpectedForeground.ToInt64())
        process_id = $processId
        process_started_at_utc = $processStartUtc.ToString('O')
    }
}

function Send-TargetRepairUnicodeText([string]$Name, [string]$Text, [IntPtr]$ExpectedRoot, [IntPtr]$ExpectedModal) {
    $index = 0
    foreach ($character in $Text.ToCharArray()) {
        $index++
        Assert-OwnedRepairModalForeground $ExpectedRoot $ExpectedModal
        if (![JumpModeNativeProbe]::SendUnicodeChar($character, $false)) {
            throw "Unicode input failed in $Name character $index"
        }
        if (![JumpModeNativeProbe]::SendUnicodeChar($character, $true)) {
            throw "Unicode key release failed in $Name character $index"
        }
    }
    if ([JumpModeNativeProbe]::ModifierIsDown()) { throw "a modifier remained held after $Name" }
    Write-Event 'target_repair_unicode_text_sent' @{
        name = $Name
        utf16_code_units = $Text.Length
        foreground_hwnd = ('0x{0:X}' -f $ExpectedModal.ToInt64())
        process_id = $processId
        process_started_at_utc = $processStartUtc.ToString('O')
    }
}

function Send-TargetRepairRootUnicodeText([string]$Name, [string]$Text, [IntPtr]$ExpectedRoot) {
    $index = 0
    foreach ($character in $Text.ToCharArray()) {
        $index++
        Assert-OwnedRepairRootForeground $ExpectedRoot
        if (![JumpModeNativeProbe]::SendUnicodeChar($character, $false)) {
            throw "Unicode input failed in $Name character $index"
        }
        if (![JumpModeNativeProbe]::SendUnicodeChar($character, $true)) {
            throw "Unicode key release failed in $Name character $index"
        }
    }
    if ([JumpModeNativeProbe]::ModifierIsDown()) { throw "a modifier remained held after $Name" }
    Write-Event 'target_repair_root_text_sent' @{
        name = $Name
        utf16_code_units = $Text.Length
        root_hwnd = ('0x{0:X}' -f $ExpectedRoot.ToInt64())
        process_id = $processId
        process_started_at_utc = $processStartUtc.ToString('O')
    }
}

function Invoke-TargetRepairRootClick(
    [string]$Name,
    [object]$Observation,
    [object]$Control,
    [IntPtr]$ExpectedRoot,
    [long]$ExpectedRequestToken
) {
    Assert-OwnedRepairRootForeground $ExpectedRoot
    $fresh = Read-JumpModeObservation
    if ($null -eq $fresh -or [long]$fresh.logic_frame_id -lt [long]$Observation.logic_frame_id -or
        $null -eq $fresh.target_repair -or
        [long]$fresh.target_repair.request_token -ne $ExpectedRequestToken -or
        [long]$fresh.target_repair.owner_revision -ne [long]$Observation.target_repair.owner_revision -or
        !$fresh.target_repair.window_visible -or $fresh.target_repair.sizing_pass) {
        throw 'repair dialog identity or current frame changed before its native control click'
    }
    $current = @($fresh.target_repair.controls | Where-Object {
        [string]$_.name -ceq [string]$Control.name -and [string]$_.response_id -ceq [string]$Control.response_id
    })
    if ($current.Count -ne 1 -or !$current[0].enabled -or !$current[0].painted) {
        throw 'repair control is no longer a unique current visible enabled response'
    }
    $ppp = [double]$fresh.pixels_per_point
    $rect = $current[0].rect
    if ($ppp -le 0 -or $null -eq $rect -or $rect.Count -ne 4) { throw 'repair response has no valid current UI geometry' }
    $clientX = [int][Math]::Round((([double]$rect[0] + [double]$rect[2]) / 2.0) * $ppp)
    $clientY = [int][Math]::Round((([double]$rect[1] + [double]$rect[3]) / 2.0) * $ppp)
    $screenX = 0; $screenY = 0
    if (![JumpModeNativeProbe]::TryClientToScreen($ExpectedRoot, $clientX, $clientY, [ref]$screenX, [ref]$screenY)) {
        throw 'could not map the current repair response into the owned root client area'
    }
    $hit = [JumpModeNativeProbe]::WindowAtScreenPoint($screenX, $screenY)
    if ($hit -eq [IntPtr]::Zero -or [int][JumpModeNativeProbe]::WindowProcessId($hit) -ne $processId -or
        [JumpModeNativeProbe]::RootWindow($hit) -ne $ExpectedRoot) {
        throw 'repair response hit test found a foreign, occluding, or unexpected window'
    }
    Assert-OwnedRepairRootForeground $ExpectedRoot
    if (![JumpModeNativeProbe]::ClickScreenPoint($screenX, $screenY)) {
        throw "SendInput click failed for current repair response $Name"
    }
    Write-Event 'target_repair_response_clicked' @{
        name = $Name
        response_id = [string]$current[0].response_id
        request_token = $ExpectedRequestToken
        logic_frame_id = [long]$fresh.logic_frame_id
        ui_pass_id = [long]$fresh.ui_pass_id
        control_screen_point = @{ x = $screenX; y = $screenY }
        hit_process_id = [JumpModeNativeProbe]::WindowProcessId($hit)
        root_hwnd = ('0x{0:X}' -f $ExpectedRoot.ToInt64())
        process_id = $processId
        process_started_at_utc = $processStartUtc.ToString('O')
    }
    return $fresh
}

function Invoke-TargetRepairActionSheetClick(
    [string]$Name,
    [object]$Observation,
    [string]$ExpectedActionId,
    [string]$ExpectedKind,
    [IntPtr]$ExpectedRoot
) {
    Assert-OwnedRepairRootForeground $ExpectedRoot
    $fresh = Read-JumpModeObservation
    if ($null -eq $fresh -or [long]$fresh.logic_frame_id -lt [long]$Observation.logic_frame_id -or
        $null -eq $fresh.target_repair_action_sheet -or
        !$fresh.target_repair_action_sheet.filter_enabled -or
        !$fresh.target_repair_action_sheet.filter_painted) {
        throw 'repair action sheet is no longer a current visible response owner'
    }
    $current = @($fresh.target_repair_action_sheet.controls | Where-Object {
        [string]$_.kind -ceq $ExpectedKind -and
            [string]$_.action_id -ceq $ExpectedActionId -and $_.enabled -and $_.painted
    })
    if ($current.Count -ne 1) { throw "repair action sheet exposed $($current.Count) current enabled $ExpectedKind responses" }
    $ppp = [double]$fresh.pixels_per_point
    $rect = $current[0].rect
    if ($ppp -le 0 -or $null -eq $rect -or $rect.Count -ne 4) { throw 'repair action response has no valid current UI geometry' }
    $clientX = [int][Math]::Round((([double]$rect[0] + [double]$rect[2]) / 2.0) * $ppp)
    $clientY = [int][Math]::Round((([double]$rect[1] + [double]$rect[3]) / 2.0) * $ppp)
    $screenX = 0; $screenY = 0
    if (![JumpModeNativeProbe]::TryClientToScreen($ExpectedRoot, $clientX, $clientY, [ref]$screenX, [ref]$screenY)) {
        throw 'could not map the current repair action response into the owned root client area'
    }
    $hit = [JumpModeNativeProbe]::WindowAtScreenPoint($screenX, $screenY)
    if ($hit -eq [IntPtr]::Zero -or [int][JumpModeNativeProbe]::WindowProcessId($hit) -ne $processId -or
        [JumpModeNativeProbe]::RootWindow($hit) -ne $ExpectedRoot) {
        throw 'repair action response hit test found a foreign, occluding, or unexpected window'
    }
    Assert-OwnedRepairRootForeground $ExpectedRoot
    if (![JumpModeNativeProbe]::ClickScreenPoint($screenX, $screenY)) { throw "SendInput click failed for $Name" }
    Write-Event 'target_repair_action_sheet_response_clicked' @{
        name = $Name
        action_id = $ExpectedActionId
        selected_action_id_before_click = $fresh.target_repair_action_sheet.selected_action_id
        response_id = [string]$current[0].response_id
        logic_frame_id = [long]$fresh.logic_frame_id
        ui_pass_id = [long]$fresh.ui_pass_id
        root_hwnd = ('0x{0:X}' -f $ExpectedRoot.ToInt64())
        process_id = $processId
        process_started_at_utc = $processStartUtc.ToString('O')
    }
    return $fresh
}

function Wait-TargetRepairOwnedPicker([IntPtr]$ExpectedRoot, [IntPtr]$PreviousForeground, [long]$RequestToken, [long]$ExpectedOwnerRevision) {
    $deadline = Get-RunDeadline ([Math]::Min($CaptureTimeoutSeconds, 20))
    while ([DateTime]::UtcNow -lt $deadline) {
        if (!(Test-RetainedProcessIdentity)) { throw 'retained app PID/birth identity changed while waiting for its repair picker' }
        $process.Refresh()
        if ($process.HasExited) {
            $script:processExitCode = $process.ExitCode
            throw "isolated app exited while opening its repair picker (exit $script:processExitCode)"
        }
        $state = Read-JumpModeObservation
        if ($null -ne $state -and $null -ne $state.target_repair -and
            [long]$state.target_repair.request_token -eq $RequestToken -and
            [long]$state.target_repair.owner_revision -eq $ExpectedOwnerRevision) {
            $foreground = [JumpModeNativeProbe]::ForegroundWindowHandle()
            if ($foreground -ne [IntPtr]::Zero -and $foreground -ne $ExpectedRoot -and
                $foreground -ne $PreviousForeground -and
                [JumpModeNativeProbe]::IsOwnedModalForeground($ExpectedRoot, [uint32]$processId)) {
                Assert-OwnedRepairModalForeground $ExpectedRoot $foreground
                return [pscustomobject]@{
                    hwnd = $foreground
                    root_hwnd = $ExpectedRoot
                    request_token = $RequestToken
                    owner_revision = $ExpectedOwnerRevision
                    egui_frame_id = [long]$state.logic_frame_id
                    egui_pass_id = [long]$state.ui_pass_id
                }
            }
        }
        Start-Sleep -Milliseconds 100
    }
    throw 'no new visible same-process native picker with the exact launcher root owner appeared before the bounded deadline'
}

function Wait-TargetRepairPickerReturn(
    [IntPtr]$ExpectedRoot,
    [IntPtr]$ExpectedModal,
    [long]$RequestToken,
    [long]$ExpectedOwnerRevision,
    [Nullable[bool]]$ExpectedReplacementMatch
) {
    $deadline = Get-RunDeadline ([Math]::Min($CaptureTimeoutSeconds, 20))
    while ([DateTime]::UtcNow -lt $deadline) {
        if (!(Test-RetainedProcessIdentity)) { throw 'retained app PID/birth identity changed while returning from its repair picker' }
        $process.Refresh()
        if ($process.HasExited) {
            $script:processExitCode = $process.ExitCode
            throw "isolated app exited while returning from its repair picker (exit $script:processExitCode)"
        }
        $root = Get-OwnedRootHandle
        $state = Read-JumpModeObservation
        if ($root -eq $ExpectedRoot -and [JumpModeNativeProbe]::ForegroundWindowHandle() -eq $ExpectedRoot -and
            [JumpModeNativeProbe]::IsOwnedModalForeground($ExpectedRoot, [uint32]$processId) -eq $false -and
            $null -ne $state -and $null -ne $state.target_repair -and
            [long]$state.target_repair.request_token -eq $RequestToken -and
            [long]$state.target_repair.owner_revision -eq $ExpectedOwnerRevision -and
            $state.target_repair.window_visible -and !$state.target_repair.sizing_pass -and
            !$state.target_repair.pending -and
            ($null -eq $ExpectedReplacementMatch -or
                $state.target_repair.replacement_matches_expected -eq [bool]$ExpectedReplacementMatch)) {
            Assert-OwnedRepairRootForeground $ExpectedRoot
            Write-Event 'target_repair_picker_returned_to_root' @{
                request_token = $RequestToken
                owner_revision = $ExpectedOwnerRevision
                expected_replacement_match = $ExpectedReplacementMatch
                returned_from_modal_hwnd = ('0x{0:X}' -f $ExpectedModal.ToInt64())
                logic_frame_id = [long]$state.logic_frame_id
                ui_pass_id = [long]$state.ui_pass_id
            }
            return $state
        }
        Start-Sleep -Milliseconds 100
    }
    throw 'repair picker did not return to the exact visible repair dialog before the bounded deadline'
}

function Wait-TargetRepairDialog([string]$Owner, [string]$ReplacementKind) {
    return Wait-JumpModeObservation {
        param($state)
        if ($null -eq $state.target_repair -or
            [string]$state.target_repair.target_owner -cne $Owner -or
            [int]$state.target_repair.record_index -ne 0 -or
            [string]$state.target_repair.replacement_kind -cne $ReplacementKind -or
            !$state.target_repair.window_visible -or $state.target_repair.sizing_pass -or
            $state.target_repair.pending) { return $false }
        foreach ($name in @('browse', 'confirm')) {
            $controls = @($state.target_repair.controls | Where-Object {
                [string]$_.name -ceq $name -and $_.enabled -and $_.painted
            })
            if ($controls.Count -ne 1) { return $false }
        }
        return $true
    } ([Math]::Min($CaptureTimeoutSeconds, 20)) "current $Owner target-repair dialog with visible enabled controls"
}

function Invoke-TargetRepairBrowseAndChoose(
    [string]$Name,
    [string]$Owner,
    [string]$ReplacementKind,
    [string]$ExpectedPath,
    [IntPtr]$Root,
    [switch]$CancelFirst
) {
    $dialog = Wait-TargetRepairDialog $Owner $ReplacementKind
    if ($CancelFirst -and $dialog.target_repair.replacement_is_empty -ne $true) {
        throw 'cancel-first file picker requires the initially empty repair draft'
    }
    $browse = @($dialog.target_repair.controls | Where-Object { $_.name -ceq 'browse' })[0]
    $previousForeground = $Root
    $null = Invoke-TargetRepairRootClick "$Name-open-picker" $dialog $browse $Root ([long]$dialog.target_repair.request_token)
    $picker = Wait-TargetRepairOwnedPicker $Root $previousForeground ([long]$dialog.target_repair.request_token) ([long]$dialog.target_repair.owner_revision)
    if ($CancelFirst) {
        Invoke-TargetRepairKeyBatch "$Name-cancel-picker" ([uint16[]]@(0x1B)) ([string[]]@('Escape')) $Root $picker.hwnd
        $returned = Wait-TargetRepairPickerReturn $Root $picker.hwnd ([long]$picker.request_token) ([long]$picker.owner_revision) $false
        if ($returned.target_repair.replacement_matches_expected -ne $false) {
            throw 'cancelled file picker changed the repair draft'
        }
        if ($returned.target_repair.replacement_is_empty -ne $true) {
            throw 'cancelled file picker did not preserve the initially empty repair draft'
        }
        $cancelledHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $script:targetRepairActionStorePath).Hash.ToLowerInvariant()
        if ($cancelledHash -cne $script:targetRepairActionStoreBeforeSha256) { throw 'cancelled file picker changed actions.json' }
        $cancelledFolderHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $script:targetRepairFolderStorePath).Hash.ToLowerInvariant()
        if ($cancelledFolderHash -cne $script:targetRepairFolderStoreBeforeSha256) { throw 'cancelled file picker changed folders.json' }
        Assert-TargetRepairAssociations $script:targetRepairOriginalActionPath $script:targetRepairOriginalFolderPath
        Write-Event 'target_repair_picker_cancel_preserved_draft' @{
            request_token = [long]$picker.request_token
            owner_revision = [long]$picker.owner_revision
            action_store_unchanged = $true
            favorite_pin_usage_associations_unchanged = $true
            process_id = $processId
        }
        $dialog = $returned
        $browse = @($dialog.target_repair.controls | Where-Object { $_.name -ceq 'browse' })[0]
        $previousForeground = $Root
        $null = Invoke-TargetRepairRootClick "$Name-reopen-picker" $dialog $browse $Root ([long]$dialog.target_repair.request_token)
        $picker = Wait-TargetRepairOwnedPicker $Root $previousForeground ([long]$dialog.target_repair.request_token) ([long]$dialog.target_repair.owner_revision)
    }

    Invoke-TargetRepairKeyBatch "$Name-focus-path-entry" ([uint16[]]@(0x12, 0x4E)) ([string[]]@('Alt', 'N')) $Root $picker.hwnd
    Invoke-TargetRepairKeyBatch "$Name-select-path-entry" ([uint16[]]@(0x11, 0x41)) ([string[]]@('Ctrl', 'A')) $Root $picker.hwnd
    Send-TargetRepairUnicodeText "$Name-type-generated-path" $ExpectedPath $Root $picker.hwnd
    Invoke-TargetRepairKeyBatch "$Name-confirm-picker-selection" ([uint16[]]@(0x0D)) ([string[]]@('Enter')) $Root $picker.hwnd
    $returned = Wait-TargetRepairPickerReturn $Root $picker.hwnd ([long]$picker.request_token) ([long]$picker.owner_revision) $true
    if ($returned.target_repair.replacement_matches_expected -ne $true) { throw 'repair picker returned without the exact expected replacement path' }
    Write-Event 'target_repair_picker_selected_generated_path' @{
        owner = $Owner
        replacement_kind = $ReplacementKind
        request_token = [long]$picker.request_token
        owner_revision = [long]$picker.owner_revision
        expected_path_verified_by_current_dialog = $true
        path_redacted = $true
        process_id = $processId
    }
    return $returned
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
    $windowClientCaptureBinding = Get-FinalSmokeCaptureClientBinding $png $captured
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
        settings_search = $capturedObservation.settings_search
        mkmacro_action_editor_open = $capturedObservation.mkmacro_action_editor_open
        pixels_per_point = $captured.pixels_per_point
        native_pixels_per_point = $captured.native_pixels_per_point
        zoom_factor = $captured.zoom_factor
        owned_client_capture_binding = $windowClientCaptureBinding
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
    if ($script:isFinalSmoke -and $CatalogNumber -notin @(5, 9)) { return $null }
    return Save-WorkflowCapture $CatalogNumber $Name $Predicate $Description
}

function Test-Phase1FinalSmokeCaptureContract {
    if (!$script:isFinalSmoke) { return }
    $selectedCatalogs = @(1..14 | Where-Object { $_ -in @(5, 9) })
    if ($script:phase1CaptureCount -ne 2 -or ($selectedCatalogs -join ',') -cne '5,9') {
        throw 'M15 FinalSmoke must request exactly two receipt captures for catalog 05 and catalog 09'
    }
    Write-Event 'm15_final_smoke_capture_contract_passed' @{ capture_count = $script:phase1CaptureCount; catalog_numbers = $selectedCatalogs; full_phase1_catalog_claim = $false }
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
    $sequenceEvent = if ($script:isFinalSmoke) { 'm15_final_smoke_native_sequence_started' } else { 'phase1_enabled_native_sequence_started' }
    Write-Event $sequenceEvent @{ expected_capture_count = $phase1CaptureCount; screen = $Screen; jump_mode_enabled = $true; full_phase1_catalog = !$script:isFinalSmoke }
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

    if ($script:isFinalSmoke) {
        $finalCatalogNumbers = @($script:phase1Captures | ForEach-Object { [int]$_.catalog_number })
        if ($script:phase1Captures.Count -ne 2 -or ($finalCatalogNumbers -join ',') -cne '5,9') {
            throw "M15 FinalSmoke saved catalog captures [$($finalCatalogNumbers -join ',')] instead of [5,9]"
        }
        $script:scenarioStatus = 'PASS'
        $script:scenarioReason = 'The M15 representative native smoke verified Dashboard catalog 05 and exact note-open catalog 09 with receipt-bound ROOT captures. It does not claim the full Phase 1 catalog or disabled-gate case.'
        $script:runStatus = 'PASS'
        $script:phase = 'm15_final_smoke_complete'
        Update-Scenarios
        Write-Event 'm15_final_smoke_assertions_passed' @{
            capture_count = $script:phase1Captures.Count
            catalog_numbers = $finalCatalogNumbers
            dashboard_action_id = $filledQuery.last_action.action_id
            dashboard_activation_count = $filledQuery.activation_count
            note_target_id = $noteTargetId
            note_action = $noteOpened.last_action
            opened_note_slugs = @($noteOpened.opened_note_slugs)
            total_activation_count = $noteOpened.activation_count
            resize = $script:finalSmokeResizeMetadata
            full_phase1_catalog_claim = $false
        }
        Write-Summary 'PASS' 'The two-capture M15 representative smoke verified the Dashboard FillQuery action and exact note 001 open after fresh native Enter input. Only catalog 05 and 09 were captured; the complete Phase 1 catalog and disabled-gate case are not claimed by this run. Closing the app and verifying its owned process tree.'
        return
    }

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
        'FileSearch' { return [pscustomobject]@{ name = 'FileSearch'; query = 'fs'; action = 'file_search:open'; args = $null; owner = 'file-search'; dialog = 'file-search' } }
        default { throw "unsupported Phase 2 surface $Surface" }
    }
}

function Test-Phase2ExactActionArgument($Actual, $Expected) {
    if ($null -eq $Expected) { return $null -eq $Actual }
    return $null -ne $Actual -and [string]$Actual -ceq [string]$Expected
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
            (Test-Phase2ExactActionArgument $_.root_action.args $Contract.args)
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
            (Test-Phase2ExactActionArgument $state.selected_action.args $Contract.args)
    }.GetNewClosure() $CaptureTimeoutSeconds "$($Contract.name) exact production opener selected from the observed hint map"
    Write-Event 'phase2_root_opener_selected' @{ surface = $Contract.name; target_id = $targetId; semantic_id = $target.semantic_id; target_generation = $target.target_generation; hint = $hint; action = $selected.selected_action.action; args = $selected.selected_action.args; query = $selected.query; activation_count = $selected.activation_count }
    Invoke-NativeKeyBatch "phase2-$($Contract.name.ToLowerInvariant())-fresh-enter-opener" ([uint16[]]@(0x0D)) ([string[]]@('Enter'))
    $opened = Wait-JumpModeObservation {
        param($state)
        (Test-Phase2OwnerSnapshot $state $Contract) -and
            [long]$state.activation_count -eq ($activationCount + 1) -and
            [string]$state.last_action.action_id -ceq [string]$Contract.action -and
            (Test-Phase2ExactActionArgument $state.last_action.args $Contract.args) -and [string]$state.last_action.source -ceq 'enter' -and
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

    $fileSearchContract = [pscustomobject]@{
        name = 'FileSearch'; query = 'fs'; action = 'file_search:open'; args = $null
        owner = 'file-search'; dialog = 'file-search'
    }
    if ($Surface -eq 'FileSearch') {
        $actualFileSearchContract = Get-Phase2SurfaceContract
        if ($actualFileSearchContract.query -cne 'fs' -or
            $actualFileSearchContract.action -cne 'file_search:open' -or
            $null -ne $actualFileSearchContract.args -or
            $actualFileSearchContract.owner -cne 'file-search' -or
            $actualFileSearchContract.dialog -cne 'file-search') {
            throw 'File Search runner contract does not match its exact production opener and navigation surface'
        }
    }
    $fileSearchTarget = [pscustomobject]@{
        target_id = 'opaque-file-search-query-target'
        label = 'Search query'
        kind = 'text_entry'
        eligible = $true
        semantic_id = [pscustomobject]@{ kind = 'named'; value = 'file-search/10:form/query' }
        hint = 'CD'
        rect = @(1, 2, 30, 40)
        clipped_rect = @(1, 2, 30, 40)
    }
    $fileSearchObservation = [pscustomobject]@{
        navigation_owner = [pscustomobject]@{ kind = 'file-search'; instance = 23 }
        root_owner_allowed = $false
        dialog_state = [pscustomobject]@{ surface = 'file-search'; instance = 23 }
        navigation_targets = @($fileSearchTarget)
    }
    $script:phase2OwnerInstance = [long]23
    $fileSearchResolved = Get-Phase2Target $fileSearchObservation $fileSearchContract 'Search query' 'text_entry' 'form/query' -RequireHint
    if ($fileSearchResolved.target_id -cne 'opaque-file-search-query-target' -or
        !(Test-Phase2OwnerSnapshot $fileSearchObservation $fileSearchContract 23)) {
        throw 'File Search helper did not require its exact current owner and query target'
    }
    $wrongFileSearchInstance = $fileSearchObservation.PSObject.Copy()
    $wrongFileSearchInstance.navigation_owner = [pscustomobject]@{ kind = 'file-search'; instance = 24 }
    $wrongFileSearchInstance.dialog_state = [pscustomobject]@{ surface = 'file-search'; instance = 24 }
    $wrongFileSearchRejected = $false
    try { $null = Get-Phase2Target $wrongFileSearchInstance $fileSearchContract 'Search query' 'text_entry' 'form/query' } catch { $wrongFileSearchRejected = $true }
    if (!$wrongFileSearchRejected -or (Test-Phase2OwnerSnapshot $wrongFileSearchInstance $fileSearchContract 23)) {
        throw 'File Search helper accepted a stale dialog instance'
    }
    $script:phase2OwnerInstance = [long]7

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
    $fileSearchRootTarget = [pscustomobject]@{
        surface = 'launcher_results'
        eligible = $true
        root_action = [pscustomobject]@{ action = $fileSearchContract.action; args = $null }
    }
    $fileSearchRootObservation = [pscustomobject]@{
        root_focused = $true
        root_owner_allowed = $true
        navigation_owner = [pscustomobject]@{ kind = 'root' }
        targets = @($fileSearchRootTarget)
    }
    if (!(Test-Phase2RootActionReady $fileSearchRootObservation $fileSearchContract)) {
        throw 'File Search root helper rejected the exact fs production opener'
    }
    $fileSearchRootTarget.root_action.action = 'file_search:start'
    if (Test-Phase2RootActionReady $fileSearchRootObservation $fileSearchContract) {
        throw 'File Search root helper accepted an action other than file_search:open'
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

    $settingsSearchContract = [pscustomobject]@{
        name = 'SettingsSearch'; query = 'settings fuzzy weight'; action = 'settings:search'; args = 'fuzzy weight'
        owner = 'settings'; dialog = 'settings'
    }
    $settingsSearchRootTarget = [pscustomobject]@{
        surface = 'launcher_results'; eligible = $true
        root_action = [pscustomobject]@{ action = 'settings:search'; args = 'fuzzy weight' }
    }
    $settingsSearchRootObservation = [pscustomobject]@{
        root_focused = $true; root_owner_allowed = $true
        navigation_owner = [pscustomobject]@{ kind = 'root' }
        targets = @($settingsSearchRootTarget)
    }
    if (!(Test-Phase2RootActionReady $settingsSearchRootObservation $settingsSearchContract)) {
        throw 'Settings Search root readiness rejected its exact nonempty production query argument'
    }
    $settingsSearchRootTarget.root_action.args = $null
    if (Test-Phase2RootActionReady $settingsSearchRootObservation $settingsSearchContract) {
        throw 'Settings Search root readiness accepted a legacy null argument in place of its required query'
    }
    $settingsSearchRootTarget.root_action.args = 'fuzzy'
    if (Test-Phase2RootActionReady $settingsSearchRootObservation $settingsSearchContract) {
        throw 'Settings Search root readiness accepted a truncated query argument'
    }

    $paintObservation = [pscustomobject]@{
        dialog_state = [pscustomobject]@{ instance = 31 }
        focused_widget_id = 'settings-response-opaque-7'
        logic_frame_id = [long]42
        ui_pass_id = [long]9
        settings_search = [pscustomobject]@{
            query_matches_expected = $true
            actual_query = 'fuzzy weight'
            highlight = [pscustomobject]@{
                control_id = 'search.fuzzy_weight'
                owner = 'application'
                owner_instance = [long]31
                response_id = 'settings-response-opaque-7'
                qa_logic_frame_id = [long]42
                qa_ui_pass_id = [long]9
                rect = @(10.0, 20.0, 30.0, 40.0)
            }
        }
    }
    if (!(Test-SettingsSearchPaint $paintObservation 'search.fuzzy_weight')) {
        throw 'Settings Search accepted-response predicate rejected a same-pass real-control highlight'
    }
    $stalePassObservation = $paintObservation.PSObject.Copy()
    $stalePassObservation.settings_search = $paintObservation.settings_search.PSObject.Copy()
    $stalePassObservation.settings_search.highlight = $paintObservation.settings_search.highlight.PSObject.Copy()
    $stalePassObservation.settings_search.highlight.qa_ui_pass_id = [long]8
    if (Test-SettingsSearchPaint $stalePassObservation 'search.fuzzy_weight') {
        throw 'Settings Search accepted a highlight receipt from a previous UI pass'
    }
    $wrongResponseObservation = $paintObservation.PSObject.Copy()
    $wrongResponseObservation.focused_widget_id = 'different-response'
    if (Test-SettingsSearchPaint $wrongResponseObservation 'search.fuzzy_weight') {
        throw 'Settings Search accepted a highlight whose response was not the current focused widget'
    }

    $focusedSliderTarget = [pscustomobject]@{
        target_id = 'settings-slider-target-opaque'
        semantic_id = [pscustomobject]@{ kind = 'named'; value = 'settings/19:search.fuzzy_weight' }
        target_generation = [long]1
        surface_identity = [pscustomobject]@{ kind = 'settings'; instance = [long]31 }
        eligible = $true
        focused = $true
        kind = 'slider'
        rect = @(10.0, 20.0, 30.0, 40.0)
        clipped_rect = @(10.0, 20.0, 30.0, 40.0)
    }
    $focusedSliderObservation = [pscustomobject]@{
        navigation_owner = [pscustomobject]@{ kind = 'settings'; instance = [long]31 }
        root_owner_allowed = $false
        dialog_state = [pscustomobject]@{ surface = 'settings'; instance = [long]31; target_generation = [long]1 }
        focused_widget_id = 'settings-response-opaque-7'
        focused_navigation_destination = 'settings-slider-target-opaque'
        settings_search = [pscustomobject]@{ query_matches_expected = $true; actual_query = 'fuzzy weight' }
        navigation_targets = @($focusedSliderTarget)
    }
    if (!(Test-SettingsSearchFocusedTarget $focusedSliderObservation $settingsSearchContract 31 'search.fuzzy_weight' 'settings-slider-target-opaque' 'settings-response-opaque-7')) {
        throw 'Settings Search focused-target predicate rejected the exact current slider response and target'
    }
    $wrongFocusedSliderResponse = $focusedSliderObservation.PSObject.Copy()
    $wrongFocusedSliderResponse.focused_widget_id = 'different-response'
    if (Test-SettingsSearchFocusedTarget $wrongFocusedSliderResponse $settingsSearchContract 31 'search.fuzzy_weight' 'settings-slider-target-opaque' 'settings-response-opaque-7') {
        throw 'Settings Search focused-target predicate accepted a different current response'
    }

    $script:phase2OwnerInstance = $savedOwnerInstance
    $script:phase2ActivationCount = $savedActivationCount
    $script:phase2Query = $savedQuery
    Write-Event 'phase2_helper_regressions_passed' @{ semantic_identity = 'structured named target'; exact_opaque_id = $resolved.target_id; observed_hint = $resolved.hint; ambiguous_target_rejected = $ambiguousRejected; exact_root_action_required = $true; ambiguous_root_action_rejected = $true; file_search_exact_open_action = $true; file_search_query_target_identity = $fileSearchResolved.target_id; file_search_wrong_instance_rejected = $wrongFileSearchRejected; explicit_empty_paste_context = $true; clipboard_text_absent = $true; exact_nonempty_notes_focus_required = $true; scoped_wait_closure_preserved_nonzero_owner_count_query = $true; settings_search_exact_query_argument = $true; settings_search_stale_ui_pass_paint_rejected = $true; settings_search_focused_target_response_required = $true }
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

function Wait-SettingsSearchPaint($Contract, [string]$ControlId, [string]$Description) {
    $waitContext = Get-Phase2WaitContext
    return Wait-JumpModeObservation {
        param($state)
        if (!(Test-Phase2OwnerSnapshot $state $Contract $waitContext.owner_instance) -or
            !$state.settings_search -or !$state.settings_search.query_matches_expected -or
            [string]$state.settings_search.actual_query -cne 'fuzzy weight') { return $false }
        $paint = $state.settings_search.highlight
        $null -ne $paint -and [string]$paint.control_id -ceq $ControlId -and
            [string]$paint.owner -ceq 'application' -and
            [long]$paint.owner_instance -eq [long]$waitContext.owner_instance -and
            [string]$paint.response_id -ceq [string]$state.focused_widget_id -and
            [long]$paint.qa_logic_frame_id -eq [long]$state.logic_frame_id -and
            [long]$paint.qa_ui_pass_id -eq [long]$state.ui_pass_id -and
            [double]$paint.rect[2] -gt [double]$paint.rect[0] -and
            [double]$paint.rect[3] -gt [double]$paint.rect[1]
    }.GetNewClosure() $CaptureTimeoutSeconds $Description
}

function Test-SettingsSearchPaint($Observation, [string]$ControlId) {
    if (!$Observation.settings_search -or !$Observation.settings_search.query_matches_expected -or
        [string]$Observation.settings_search.actual_query -cne 'fuzzy weight') { return $false }
    $paint = $Observation.settings_search.highlight
    return $null -ne $paint -and [string]$paint.control_id -ceq $ControlId -and
        [string]$paint.owner -ceq 'application' -and
        [long]$paint.owner_instance -eq [long]$Observation.dialog_state.instance -and
        [string]$paint.response_id -ceq [string]$Observation.focused_widget_id -and
        [long]$paint.qa_logic_frame_id -eq [long]$Observation.logic_frame_id -and
        [long]$paint.qa_ui_pass_id -eq [long]$Observation.ui_pass_id -and
        [double]$paint.rect[2] -gt [double]$paint.rect[0] -and
        [double]$paint.rect[3] -gt [double]$paint.rect[1]
}

function Test-SettingsSearchFocusedTarget($Observation, $Contract, [long]$ExpectedInstance, [string]$ControlId, [string]$TargetId, [string]$ResponseId) {
    if (!(Test-Phase2OwnerSnapshot $Observation $Contract $ExpectedInstance) -or
        !$Observation.settings_search -or !$Observation.settings_search.query_matches_expected -or
        [string]$Observation.settings_search.actual_query -cne 'fuzzy weight' -or
        [string]$Observation.focused_widget_id -cne $ResponseId -or
        [string]$Observation.focused_navigation_destination -cne $TargetId) { return $false }

    $targetMatches = @($Observation.navigation_targets | Where-Object {
        [string]$_.target_id -ceq $TargetId -and
            $_.eligible -and $_.focused -and $_.kind -ceq 'slider' -and
            $_.surface_identity.kind -ceq 'settings' -and
            [long]$_.surface_identity.instance -eq $ExpectedInstance -and
            [long]$_.target_generation -eq [long]$Observation.dialog_state.target_generation -and
            (Test-Phase2SemanticIdentity $_ $ControlId) -and
            (Test-Phase2TargetGeometry $_)
    })
    return $targetMatches.Count -eq 1
}

function Invoke-SettingsSearchNativeWorkflow {
    $contract = [pscustomobject]@{
        name = 'SettingsSearch'
        query = 'settings fuzzy weight'
        action = 'settings:search'
        args = 'fuzzy weight'
        owner = 'settings'
        dialog = 'settings'
    }
    Write-Event 'settings_search_native_started' @{ query = $contract.query; action = $contract.action; args = $contract.args; settings_search_query = 'fuzzy weight'; expected_capture_count = $phase2CaptureCount }
    $opened = Invoke-Phase2RootOpener $contract
    $waitContext = Get-Phase2WaitContext
    $baselineWeight = [double]$opened.effective_fuzzy_weight
    $committedWeight = [double]$opened.dialog_state.settings_committed_fuzzy_weight
    if ($opened.dialog_state.settings_draft_jump_mode -ne $true -or
        $opened.dialog_state.settings_committed_jump_mode -ne $true) {
        throw 'Settings Search fixture did not retain enabled committed and draft Jump Mode'
    }

    $sliderResult = Complete-Phase2HintTarget $contract 'Fuzzy match weight' 'button' 'settings.search.result/search.fuzzy_weight'
    Invoke-NativeKeyBatch 'settings-search-slider-result-fresh-enter' ([uint16[]]@(0x0D)) ([string[]]@('Enter'))
    $sliderPaint = Wait-SettingsSearchPaint $contract 'search.fuzzy_weight' 'Settings search result handoff to the real Fuzzy weight slider response'
    if ([long]$sliderPaint.activation_count -ne $waitContext.activation_count -or
        [double]$sliderPaint.dialog_state.settings_draft_fuzzy_weight -ne $baselineWeight -or
        [double]$sliderPaint.dialog_state.settings_committed_fuzzy_weight -ne $committedWeight) {
        throw 'Fuzzy weight search handoff changed an action count or setting value before native editing'
    }
    $sliderTargetId = [string]$sliderPaint.settings_search.highlight.response_id
    $sliderNavigationTargets = @($sliderPaint.navigation_targets | Where-Object {
        $_.eligible -and $_.focused -and $_.kind -ceq 'slider' -and
            $_.surface_identity.kind -ceq 'settings' -and
            [long]$_.surface_identity.instance -eq $waitContext.owner_instance -and
            [long]$_.target_generation -eq [long]$sliderPaint.dialog_state.target_generation -and
            (Test-Phase2SemanticIdentity $_ 'search.fuzzy_weight') -and
            (Test-Phase2TargetGeometry $_)
    })
    if ($sliderNavigationTargets.Count -ne 1) {
        throw "Settings Search observed $($sliderNavigationTargets.Count) current focused slider targets for the painted Fuzzy weight response"
    }
    $sliderNavigationTargetId = [string]$sliderNavigationTargets[0].target_id
    $null = Save-Phase2Capture 1 'settings_search_slider_focus' {
        param($state)
        (Test-Phase2OwnerSnapshot $state $contract $waitContext.owner_instance) -and
            [long]$state.activation_count -eq [long]$waitContext.activation_count -and
            [double]$state.dialog_state.settings_draft_fuzzy_weight -eq $baselineWeight -and
            [double]$state.dialog_state.settings_committed_fuzzy_weight -eq $committedWeight -and
            (Test-SettingsSearchPaint $state 'search.fuzzy_weight') -and
            [string]$state.settings_search.highlight.response_id -ceq $sliderTargetId
    }.GetNewClosure() 'the exact current Fuzzy weight slider response is highlighted and focused'

    Invoke-NativeKeyBatch 'settings-search-slider-right-draft-only' ([uint16[]]@(0x27)) ([string[]]@('Right'))
    $sliderChanged = Wait-JumpModeObservation {
        param($state)
        (Test-Phase2OwnerSnapshot $state $contract $waitContext.owner_instance) -and
            [long]$state.activation_count -eq [long]$waitContext.activation_count -and
            [double]$state.dialog_state.settings_draft_fuzzy_weight -gt $baselineWeight -and
            [double]$state.dialog_state.settings_committed_fuzzy_weight -eq $committedWeight
    }.GetNewClosure() $CaptureTimeoutSeconds 'native Right changes only the Fuzzy weight draft'
    if ([double]$sliderChanged.dialog_state.settings_draft_fuzzy_weight -gt 5.0) { throw 'Fuzzy weight draft exceeded its bounded range' }
    $sliderDraftWeight = [double]$sliderChanged.dialog_state.settings_draft_fuzzy_weight
    $null = Save-Phase2Capture 2 'settings_search_slider_draft' {
        param($state)
        (Test-Phase2OwnerSnapshot $state $contract $waitContext.owner_instance) -and
            [long]$state.activation_count -eq [long]$waitContext.activation_count -and
            [double]$state.dialog_state.settings_draft_fuzzy_weight -eq $sliderDraftWeight -and
            [double]$state.dialog_state.settings_committed_fuzzy_weight -eq $committedWeight -and
            (Test-SettingsSearchFocusedTarget $state $contract $waitContext.owner_instance 'search.fuzzy_weight' $sliderNavigationTargetId $sliderTargetId)
    }.GetNewClosure() 'the changed slider draft remains separate from committed settings under the same current focused slider target and response'

    $numericResult = Complete-Phase2HintTarget $contract 'Fuzzy match weight value' 'button' 'settings.search.result/search.fuzzy_weight/value'
    Invoke-NativeKeyBatch 'settings-search-numeric-result-fresh-enter' ([uint16[]]@(0x0D)) ([string[]]@('Enter'))
    $numericPaint = Wait-SettingsSearchPaint $contract 'search.fuzzy_weight/value' 'Settings search result handoff to the real Fuzzy weight numeric response'
    if ([long]$numericPaint.activation_count -ne $waitContext.activation_count -or
        [double]$numericPaint.dialog_state.settings_draft_fuzzy_weight -ne $sliderDraftWeight -or
        [double]$numericPaint.dialog_state.settings_committed_fuzzy_weight -ne $committedWeight) {
        throw 'Fuzzy weight numeric handoff changed an action count or setting value before native editing'
    }
    $numericTargetId = [string]$numericPaint.settings_search.highlight.response_id
    $null = Save-Phase2Capture 3 'settings_search_numeric_focus' {
        param($state)
        (Test-Phase2OwnerSnapshot $state $contract $waitContext.owner_instance) -and
            [long]$state.activation_count -eq [long]$waitContext.activation_count -and
            [double]$state.dialog_state.settings_draft_fuzzy_weight -eq $sliderDraftWeight -and
            [double]$state.dialog_state.settings_committed_fuzzy_weight -eq $committedWeight -and
            (Test-SettingsSearchPaint $state 'search.fuzzy_weight/value') -and
            [string]$state.settings_search.highlight.response_id -ceq $numericTargetId
    }.GetNewClosure() 'the exact Fuzzy weight numeric response is highlighted and focused'

    Invoke-NativeKeyBatch 'settings-search-numeric-up-draft-only' ([uint16[]]@(0x26)) ([string[]]@('Up'))
    $numericChanged = Wait-JumpModeObservation {
        param($state)
        (Test-Phase2OwnerSnapshot $state $contract $waitContext.owner_instance) -and
            [long]$state.activation_count -eq [long]$waitContext.activation_count -and
            [double]$state.dialog_state.settings_draft_fuzzy_weight -gt $sliderDraftWeight -and
            [double]$state.dialog_state.settings_committed_fuzzy_weight -eq $committedWeight
    }.GetNewClosure() $CaptureTimeoutSeconds 'native Up changes only the numeric Fuzzy weight draft'
    if ([double]$numericChanged.dialog_state.settings_draft_fuzzy_weight -gt 5.0) { throw 'Fuzzy weight numeric draft exceeded its bounded range' }

    $cancel = Complete-Phase2HintTarget $contract 'Cancel' 'button' 'footer.cancel'
    Invoke-NativeKeyBatch 'settings-search-cancel-draft-fresh-enter' ([uint16[]]@(0x0D)) ([string[]]@('Enter'))
    $closed = Wait-JumpModeObservation {
        param($state)
        $state.navigation_owner.kind -ceq 'root' -and $state.root_focused -and
            !$state.active_session -and [long]$state.activation_count -eq [long]$waitContext.activation_count -and
            [bool]$state.jump_mode_enabled -and
            [double]$state.effective_fuzzy_weight -eq $baselineWeight -and
            $state.settings_search -and $state.settings_search.query_matches_expected -and
            [string]$state.settings_search.actual_query -ceq 'fuzzy weight' -and
            $null -eq $state.settings_search.highlight
    }.GetNewClosure() $CaptureTimeoutSeconds 'Settings Cancel closes the dirty draft without applying it and retires the real-control highlight'
    Write-Event 'settings_search_cancel_verified' @{
        activation_count = $closed.activation_count
        owner_instance = $waitContext.owner_instance
        fuzzy_weight_initial = $baselineWeight
        fuzzy_weight_slider_draft = $sliderDraftWeight
        fuzzy_weight_numeric_draft = [double]$numericChanged.dialog_state.settings_draft_fuzzy_weight
        fuzzy_weight_effective_after_cancel = [double]$closed.effective_fuzzy_weight
        actual_response_ids = @($sliderTargetId, $numericTargetId)
        captures = @($phase2Captures | ForEach-Object { $_.capture_index })
        jump_mode_off_native = 'NOT_ATTEMPTED; Jump-off behavior remains covered by deterministic current-response tests'
    }
    if ($phase2Captures.Count -ne 3) { throw 'Settings Search workflow did not save all three receipt-bound ROOT/F11 captures' }
    if (!(Test-Path -LiteralPath $prebuiltSettingsPath -PathType Leaf)) {
        throw 'isolated settings.json disappeared before the live no-save check after Settings Search Cancel'
    }
    $script:settingsSearchNavigationSha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $prebuiltSettingsPath).Hash.ToLowerInvariant()
    if ($script:settingsSearchNavigationSha256 -cne $script:prebuiltSettingsPreInputSha256) {
        throw 'the isolated settings.json changed during Settings Search navigation before app close'
    }
    $script:settingsSearchNavigationHashVerified = $true
    Write-Event 'settings_search_settings_file_unchanged_while_app_open' @{
        sha256 = $script:settingsSearchNavigationSha256
        before_launch_sha256 = $prebuiltSettingsSha256
        before_first_input_sha256 = $script:prebuiltSettingsPreInputSha256
        verified_after_cancel = $true
        verified_while_owned_app_running = $true
    }
}

function Ensure-TargetRepairQueryFocus([IntPtr]$Root) {
    $baseline = Wait-JumpModeObservation {
        param($state)
        $state.root_focused -and $state.root_owner_allowed -and
            [string]$state.navigation_owner.kind -ceq 'root' -and !$state.active_session
    } ([Math]::Min($CaptureTimeoutSeconds, 20)) 'TargetRepair exact owned ROOT owner before query interaction'
    Assert-OwnedForeground
    Assert-OwnedRepairRootForeground $Root
    if ($baseline.query_focused) { return $baseline }
    if (!$baseline.jump_mode_enabled) { throw 'TargetRepair cannot focus its current query with Jump Mode disabled' }

    $queryBefore = [string]$baseline.query
    $activationBefore = [long]$baseline.activation_count
    Invoke-TargetRepairKeyBatch 'target-repair-query-focus-leader' ([uint16[]]@(0x10, 0x20)) ([string[]]@('Shift', 'Space')) $Root $Root
    $active = Wait-JumpModeObservation {
        param($state)
        $state.root_focused -and $state.root_owner_allowed -and
            [string]$state.navigation_owner.kind -ceq 'root' -and
            $state.jump_mode_enabled -and $state.active_session -and
            [string]$state.query -ceq $queryBefore -and
            [long]$state.activation_count -eq $activationBefore
    } ([Math]::Min($CaptureTimeoutSeconds, 10)) 'TargetRepair current ROOT query hint map without activation'
    $queryTarget = Get-Phase1QueryTarget $active
    $targetId = [string]$queryTarget.target_id
    foreach ($character in ([string]$queryTarget.hint).ToCharArray()) {
        Invoke-TargetRepairKeyBatch 'target-repair-query-focus-hint' ([uint16[]]@([char]$character)) ([string[]]@([string]$character)) $Root $Root
    }
    return Wait-JumpModeObservation {
        param($state)
        $state.root_focused -and $state.root_owner_allowed -and
            [string]$state.navigation_owner.kind -ceq 'root' -and !$state.active_session -and
            $null -ne $state.outcome -and $state.outcome.kind -ceq 'completed' -and
            [string]$state.outcome.target_id -ceq $targetId -and
            [string]$state.focused_destination -ceq $targetId -and $state.query_focused -and
            [string]$state.query -ceq $queryBefore -and
            [long]$state.activation_count -eq $activationBefore
    } ([Math]::Min($CaptureTimeoutSeconds, 10)) 'TargetRepair observed query Response focus handoff'
}

function Set-TargetRepairQuery([IntPtr]$Root, [string]$Query, [string]$ResultLabel) {
    $before = Ensure-TargetRepairQueryFocus $Root
    Assert-OwnedRepairRootForeground $Root
    if (![string]::IsNullOrEmpty([string]$before.query)) {
        Invoke-TargetRepairKeyBatch 'target-repair-query-select-all' ([uint16[]]@(0x11, 0x41)) ([string[]]@('Ctrl', 'A')) $Root $Root
        Invoke-TargetRepairKeyBatch 'target-repair-query-clear' ([uint16[]]@(0x08)) ([string[]]@('Backspace')) $Root $Root
    }
    Send-TargetRepairRootUnicodeText 'target-repair-query-type-generated-label' $Query $Root
    return Wait-JumpModeObservation {
        param($state)
        if (!$state.root_focused -or !$state.root_owner_allowed -or
            [string]$state.navigation_owner.kind -cne 'root' -or $state.active_session -or
            !$state.query_focused -or [string]$state.query -cne $Query) { return $false }
        $matches = @($state.targets | Where-Object {
            $_.surface -ceq 'launcher_results' -and $_.eligible -and
                $null -ne $_.root_action -and [string]$_.label -ceq $ResultLabel
        })
        return $matches.Count -eq 1
    } ([Math]::Min($CaptureTimeoutSeconds, 20)) "TargetRepair exact ROOT result '$ResultLabel' for its generated query"
}

function Focus-TargetRepairRootResult([IntPtr]$Root, [string]$Query, [string]$ResultLabel) {
    $active = Ensure-TargetRepairQueryFocus $Root
    if ([string]$active.query -cne $Query) { throw 'TargetRepair query changed before selecting its saved result' }
    $activationBefore = [long]$active.activation_count
    Invoke-TargetRepairKeyBatch 'target-repair-result-hint-leader' ([uint16[]]@(0x10, 0x20)) ([string[]]@('Shift', 'Space')) $Root $Root
    $hinted = Wait-JumpModeObservation {
        param($state)
        $state.root_focused -and $state.root_owner_allowed -and $state.navigation_owner.kind -ceq 'root' -and
            $state.active_session -and [string]$state.query -ceq $Query -and
            [long]$state.activation_count -eq $activationBefore
    } ([Math]::Min($CaptureTimeoutSeconds, 10)) 'TargetRepair actual ROOT result hint map'
    $matches = @($hinted.targets | Where-Object {
        $_.surface -ceq 'launcher_results' -and $_.eligible -and
            $null -ne $_.root_action -and [string]$_.label -ceq $ResultLabel -and
            [string]$_.hint -match '^[A-Z]+$'
    })
    if ($matches.Count -ne 1) { throw "TargetRepair mapped $($matches.Count) exact '$ResultLabel' ROOT result hints" }
    $target = $matches[0]
    foreach ($character in ([string]$target.hint).ToCharArray()) {
        Invoke-TargetRepairKeyBatch 'target-repair-result-hint' ([uint16[]]@([char]$character)) ([string[]]@([string]$character)) $Root $Root
    }
    $selected = Wait-JumpModeObservation {
        param($state)
        !$state.active_session -and $state.root_focused -and $state.root_owner_allowed -and
            $state.navigation_owner.kind -ceq 'root' -and $state.query_focused -and
            [string]$state.query -ceq $Query -and
            $null -ne $state.outcome -and $state.outcome.kind -ceq 'completed' -and
            [string]$state.outcome.target_id -ceq [string]$target.target_id -and
            [string]$state.focused_destination -ceq [string]$state.query_target_id -and
            [string]$state.selected_target_id -ceq [string]$target.target_id -and
            [long]$state.activation_count -eq $activationBefore
    } ([Math]::Min($CaptureTimeoutSeconds, 10)) "TargetRepair focus-only completion for '$ResultLabel'"
    Write-Event 'target_repair_root_result_focused' @{
        query = $Query
        label = $ResultLabel
        target_id = [string]$target.target_id
        target_generation = [long]$target.target_generation
        hint = [string]$target.hint
        selected_index = $selected.selected_index
        activation_count_unchanged = $true
        logic_frame_id = [long]$selected.logic_frame_id
        ui_pass_id = [long]$selected.ui_pass_id
    }
    return $selected
}

function Open-TargetRepairActionSheet([IntPtr]$Root, [string]$Query, [string]$ResultLabel, [string]$Kind, [string]$ActionId) {
    $selected = Focus-TargetRepairRootResult $Root $Query $ResultLabel
    if (!$selected.query_focused) { throw 'TargetRepair result focus did not retain the production query shortcut owner' }
    $activationBefore = [long]$selected.activation_count
    Invoke-TargetRepairKeyBatch "target-repair-open-$Kind-action-sheet" ([uint16[]]@(0x11, 0x0D)) ([string[]]@('Ctrl', 'Enter')) $Root $Root
    $sheet = Wait-JumpModeObservation {
        param($state)
        $view = $state.target_repair_action_sheet
        if ($null -eq $view -or !$view.filter_enabled -or !$view.filter_painted -or !$view.filter_focused -or
            [long]$state.activation_count -ne $activationBefore) { return $false }
        $matches = @($view.controls | Where-Object {
            [string]$_.kind -ceq $Kind -and [string]$_.action_id -ceq $ActionId -and
                $_.enabled -and $_.painted
        })
        return $matches.Count -eq 1
    } ([Math]::Min($CaptureTimeoutSeconds, 15)) "TargetRepair visible exact $Kind action-sheet Response"
    $control = @($sheet.target_repair_action_sheet.controls | Where-Object {
        [string]$_.kind -ceq $Kind -and [string]$_.action_id -ceq $ActionId
    })[0]
    $null = Invoke-TargetRepairActionSheetClick "target-repair-activate-$Kind-repair-intent" $sheet $ActionId $Kind $Root
    $dialog = Wait-TargetRepairDialog $(if ($Kind -ceq 'custom_action') { 'custom_action' } else { 'folder' }) $(if ($Kind -ceq 'custom_action') { 'file' } else { 'folder' })
    $intent = Wait-JumpModeObservation {
        param($state)
        $null -ne $state.target_repair -and
            [string]$state.target_repair.target_owner -ceq $(if ($Kind -ceq 'custom_action') { 'custom_action' } else { 'folder' }) -and
            [long]$state.target_repair.request_token -eq [long]$dialog.target_repair.request_token -and
            [long]$state.activation_count -eq $activationBefore
    } ([Math]::Min($CaptureTimeoutSeconds, 10)) "TargetRepair exact non-primary $Kind UI intent"
    if ([long]$intent.activation_count -ne $activationBefore) {
        throw 'TargetRepair UI intent changed the primary action activation counter'
    }
    Write-Event 'target_repair_action_sheet_intent_observed' @{
        action_id = $ActionId
        control_response_id = [string]$control.response_id
        primary_activation_count_unchanged = $true
        activation_count = [long]$intent.activation_count
        request_token = [long]$dialog.target_repair.request_token
        target_owner = [string]$dialog.target_repair.target_owner
    }
    return $dialog
}

function Complete-TargetRepairChildAndCloseParentEditor(
    [string]$Name,
    [long]$RequestToken,
    [long]$OwnerRevision,
    [long]$ExpectedActivationCount,
    [IntPtr]$Root
) {
    $completed = Wait-JumpModeObservation {
        param($state)
        [long]$state.activation_count -eq $ExpectedActivationCount -and
            $state.app_editor_open -and
            ($null -eq $state.target_repair -or
                ([long]$state.target_repair.request_token -eq $RequestToken -and
                    [long]$state.target_repair.owner_revision -eq $OwnerRevision -and
                    $state.target_repair.pending))
    } ([Math]::Min($CaptureTimeoutSeconds, 20)) "$Name repair completion or exact submitted pending state"
    if ($null -ne $completed.target_repair) {
        $completed = Wait-JumpModeObservation {
            param($state)
            $null -eq $state.target_repair -and $state.app_editor_open -and
                [long]$state.activation_count -eq $ExpectedActivationCount
        } ([Math]::Min($CaptureTimeoutSeconds, 20)) "$Name child-dialog completion while parent Editor remains open"
    }
    if (!$completed.app_editor_open -or $null -ne $completed.target_repair -or
        [long]$completed.activation_count -ne $ExpectedActivationCount) {
        throw "$Name completion did not retain the parent Editor without a primary action activation"
    }
    Invoke-TargetRepairKeyBatch "$Name-close-parent-editor" ([uint16[]]@(0x1B)) ([string[]]@('Escape')) $Root $Root
    $root = Wait-JumpModeObservation {
        param($state)
        !$state.app_editor_open -and $null -eq $state.target_repair -and
            $state.root_focused -and $state.root_owner_allowed -and
            $state.navigation_owner.kind -ceq 'root' -and
            [long]$state.activation_count -eq $ExpectedActivationCount
    } ([Math]::Min($CaptureTimeoutSeconds, 15)) "$Name parent Editor closed to the exact ROOT owner"
    Write-Event 'target_repair_parent_editor_closed' @{
        operation = $Name
        request_token = $RequestToken
        owner_revision = $OwnerRevision
        primary_activation_count_unchanged = $true
        activation_count = [long]$root.activation_count
    }
    return $root
}

function Assert-TargetRepairActionStore([string]$ExpectedPath) {
    if (!(Test-Path -LiteralPath $script:targetRepairActionStorePath -PathType Leaf)) { throw 'TargetRepair actions.json is missing after repair' }
    $records = @(Get-Content -Raw -LiteralPath $script:targetRepairActionStorePath | ConvertFrom-Json)
    $fixture = $script:targetRepairFixtureManifest
    if ($records.Count -ne 1 -or
        [string]$records[0].label -cne [string]$fixture.action_label -or
        [string]$records[0].desc -cne [string]$fixture.action_description -or
        [string]$records[0].action -cne $ExpectedPath -or
        [string]$records[0].args -cne [string]$fixture.action_args) {
        throw 'TargetRepair actions.json did not preserve the exact generated record metadata, arguments, and chosen path'
    }
}

function Assert-TargetRepairFolderStore([string]$ExpectedPath) {
    if (!(Test-Path -LiteralPath $script:targetRepairFolderStorePath -PathType Leaf)) { throw 'TargetRepair folders.json is missing after repair' }
    $records = @(Get-Content -Raw -LiteralPath $script:targetRepairFolderStorePath | ConvertFrom-Json)
    $fixture = $script:targetRepairFixtureManifest
    if ($records.Count -ne 1 -or
        [string]$records[0].label -cne [string]$fixture.folder_label -or
        [string]$records[0].path -cne $ExpectedPath -or
        [string]$records[0].alias -cne [string]$fixture.folder_alias) {
        throw 'TargetRepair folders.json did not preserve the exact generated label/alias and chosen path'
    }
}

function Assert-TargetRepairAssociations([string]$ActionPath, [string]$FolderPath) {
    $fixture = $script:targetRepairFixtureManifest
    $associations = $fixture.associations
    $favorites = @(Get-Content -Raw -LiteralPath $script:targetRepairFavoriteStorePath | ConvertFrom-Json)
    $pins = @(Get-Content -Raw -LiteralPath $script:targetRepairPinStorePath | ConvertFrom-Json)
    $usage = @(Get-Content -Raw -LiteralPath $script:targetRepairUsageStorePath | ConvertFrom-Json)
    if ($favorites.Count -ne 3 -or
        [string]$favorites[0].label -cne [string]$associations.action_favorite_label -or
        [string]$favorites[0].action -cne $ActionPath -or
        [string]$favorites[0].args -cne [string]$fixture.action_args -or
        [string]$favorites[1].label -cne [string]$associations.folder_favorite_label -or
        [string]$favorites[1].action -cne $FolderPath -or
        $null -ne $favorites[1].args -or
        [string]$favorites[2].label -cne [string]$associations.unrelated_favorite_label -or
        [string]$favorites[2].action -cne $script:targetRepairUnrelatedPath -or
        [string]$favorites[2].args -cne '--leave-unrelated') {
        throw 'TargetRepair favorite references did not preserve exact changed and unrelated action associations'
    }
    if ($pins.Count -ne 3 -or
        [string]$pins[0].action_id -cne $ActionPath -or
        [string]$pins[0].args -cne [string]$fixture.action_args -or
        [string]$pins[0].label -cne [string]$associations.action_favorite_label -or
        [string]$pins[0].query -cne [string]$associations.action_pin_query -or
        [string]$pins[0].desc -cne 'Inert saved Target Repair association.' -or
        [int64]$pins[0].timestamp -ne 31 -or
        [string]$pins[1].action_id -cne $FolderPath -or
        $null -ne $pins[1].args -or
        [string]$pins[1].label -cne [string]$associations.folder_favorite_label -or
        [string]$pins[1].query -cne [string]$associations.folder_pin_query -or
        [string]$pins[1].desc -cne 'Inert saved Target Repair association.' -or
        [int64]$pins[1].timestamp -ne 32 -or
        [string]$pins[2].action_id -cne $script:targetRepairUnrelatedPath -or
        [string]$pins[2].args -cne '--leave-unrelated' -or
        [string]$pins[2].label -cne [string]$associations.unrelated_favorite_label -or
        [string]$pins[2].query -cne [string]$associations.unrelated_pin_query -or
        [string]$pins[2].desc -cne 'Inert saved Target Repair association.' -or
        [int64]$pins[2].timestamp -ne 33) {
        throw 'TargetRepair pin references did not preserve exact path/args/query and unrelated rows'
    }
    $actionUsage = @($usage | Where-Object { [string]$_.action -ceq $ActionPath })
    $folderUsage = @($usage | Where-Object { [string]$_.action -ceq $FolderPath })
    $unrelatedUsage = @($usage | Where-Object { [string]$_.action -ceq $script:targetRepairUnrelatedPath })
    if ($usage.Count -ne 3 -or $actionUsage.Count -ne 1 -or
        [uint32]$actionUsage[0].count -ne [uint32]$associations.action_usage_count -or
        $folderUsage.Count -ne 1 -or
        [uint32]$folderUsage[0].count -ne [uint32]$associations.folder_usage_count -or
        $unrelatedUsage.Count -ne 1 -or
        [uint32]$unrelatedUsage[0].count -ne [uint32]$associations.unrelated_usage_count) {
        throw 'TargetRepair usage counts did not move to exact repaired paths or unrelated usage changed'
    }
}

function Get-NoteReplaceCurrentPanel([object]$Observation, [string]$ExpectedIdentity = '') {
    if ($null -eq $Observation -or $null -eq $Observation.note_replace) { return $null }
    $paint = $Observation.note_replace
    if (!$paint.open -or !$paint.expected_panel_match -or
        [string]::IsNullOrWhiteSpace([string]$paint.panel_identity) -or
        ($ExpectedIdentity -and [string]$paint.panel_identity -cne $ExpectedIdentity)) {
        return $null
    }
    return $paint
}

function Get-NoteReplaceRootPaint([object]$Observation) {
    if ($null -eq $Observation -or $null -eq $Observation.note_replace_root) { return $null }
    $paint = $Observation.note_replace_root
    if (!$paint.root_owner_allowed -or
        [long]$paint.logic_frame_id -ne [long]$Observation.logic_frame_id -or
        [long]$paint.ui_pass_id -ne [long]$Observation.ui_pass_id -or
        [long]$paint.ui_pass_index -ne [long]$Observation.ui_pass_index -or
        $null -eq $paint.query) { return $null }
    return $paint
}

function Get-NoteReplaceTextSha256([string]$Text) {
    $bytes = [Text.UTF8Encoding]::new($false, $true).GetBytes($Text)
    return [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($bytes)).ToLowerInvariant()
}

function Assert-NoteReplaceRoot([IntPtr]$ExpectedRoot, [string]$ExpectedPanelIdentity = '') {
    Assert-OwnedRepairRootForeground $ExpectedRoot
    $state = Read-JumpModeObservation
    $ownerAllowed = if ($ExpectedPanelIdentity) {
        $null -ne (Get-NoteReplaceCurrentPanel $state $ExpectedPanelIdentity) -and
            [bool]$state.note_replace.root_owner_allowed
    } else {
        $null -ne (Get-NoteReplaceRootPaint $state)
    }
    if ($null -eq $state -or !$state.root_focused -or !$ownerAllowed) {
        throw 'NoteReplace input refused because the current owned ROOT observation is unavailable or ineligible'
    }
    if ($ExpectedPanelIdentity -and $null -eq (Get-NoteReplaceCurrentPanel $state $ExpectedPanelIdentity)) {
        throw 'NoteReplace input refused because the current note panel identity changed or closed'
    }
    return $state
}

function Invoke-NoteReplaceKeyBatch(
    [string]$Name,
    [uint16[]]$Keys,
    [string[]]$KeyLabels,
    [IntPtr]$ExpectedRoot,
    [string]$ExpectedPanelIdentity = '',
    [string]$FocusedControl = ''
) {
    if ($Keys.Count -eq 0 -or $Keys.Count -ne $KeyLabels.Count) { throw 'NoteReplace key batch is empty or malformed' }
    $state = Assert-NoteReplaceRoot $ExpectedRoot $ExpectedPanelIdentity
    if ($FocusedControl) {
        if ($ExpectedPanelIdentity) {
            $paint = Get-NoteReplaceCurrentPanel $state $ExpectedPanelIdentity
            $property = $paint.controls.PSObject.Properties[$FocusedControl]
            if ($null -eq $property -or $null -eq $property.Value -or !$property.Value.enabled -or !$property.Value.focused) {
                throw "NoteReplace keyboard input refused because the current $FocusedControl response is not focused"
            }
        } elseif (!$state.query_focused) {
            throw 'NoteReplace launcher input refused because the current query TextEntry is not focused'
        }
    }
    $pressed = [Collections.Generic.List[uint16]]::new()
    $sendFailure = $null
    try {
        foreach ($key in $Keys) {
            if (![JumpModeNativeProbe]::ModifierIsDown() -and $pressed.Count -eq 0 -and $ExpectedPanelIdentity) {
                $null = Assert-NoteReplaceRoot $ExpectedRoot $ExpectedPanelIdentity
            }
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
    if ([JumpModeNativeProbe]::ModifierIsDown()) { throw "a modifier remained held after $Name" }
    Write-Event 'note_replace_key_batch_sent' @{
        name = $Name
        keys = $KeyLabels
        root_hwnd = ('0x{0:X}' -f $ExpectedRoot.ToInt64())
        panel_identity = if ($ExpectedPanelIdentity) { $ExpectedPanelIdentity } else { $null }
        logic_frame_id = [long]$state.logic_frame_id
        ui_pass_id = [long]$state.ui_pass_id
    }
}

function Send-NoteReplaceUnicodeText(
    [string]$Name,
    [string]$Text,
    [IntPtr]$ExpectedRoot,
    [string]$ExpectedPanelIdentity = '',
    [string]$FocusedControl = ''
) {
    $state = Assert-NoteReplaceRoot $ExpectedRoot $ExpectedPanelIdentity
    if ($FocusedControl) {
        if ($ExpectedPanelIdentity) {
            $paint = Get-NoteReplaceCurrentPanel $state $ExpectedPanelIdentity
            $property = $paint.controls.PSObject.Properties[$FocusedControl]
            if ($null -eq $property -or $null -eq $property.Value -or !$property.Value.enabled -or !$property.Value.focused) {
                throw "NoteReplace text input refused because the current $FocusedControl response is not focused"
            }
        } elseif (!$state.query_focused) {
            throw 'NoteReplace text input refused because the current query TextEntry is not focused'
        }
    }
    $index = 0
    foreach ($character in $Text.ToCharArray()) {
        $index++
        $null = Assert-NoteReplaceRoot $ExpectedRoot $ExpectedPanelIdentity
        if (![JumpModeNativeProbe]::SendUnicodeChar($character, $false)) {
            throw "Unicode input failed in $Name character $index"
        }
        if (![JumpModeNativeProbe]::SendUnicodeChar($character, $true)) {
            throw "Unicode key release failed in $Name character $index"
        }
    }
    if ([JumpModeNativeProbe]::ModifierIsDown()) { throw "a modifier remained held after $Name" }
    Write-Event 'note_replace_text_sent' @{
        name = $Name
        utf16_code_units = $Text.Length
        root_hwnd = ('0x{0:X}' -f $ExpectedRoot.ToInt64())
        panel_identity = if ($ExpectedPanelIdentity) { $ExpectedPanelIdentity } else { $null }
        logic_frame_id = [long]$state.logic_frame_id
        ui_pass_id = [long]$state.ui_pass_id
    }
}

function Test-NoteReplaceRect([object]$Control) {
    if ($null -eq $Control -or $null -eq $Control.rect -or $null -eq $Control.clipped_rect -or
        $Control.rect.Count -ne 4 -or $Control.clipped_rect.Count -ne 4) { return $false }
    $r = @($Control.rect | ForEach-Object { [double]$_ })
    $c = @($Control.clipped_rect | ForEach-Object { [double]$_ })
    return $r[2] -gt $r[0] -and $r[3] -gt $r[1] -and
        $c[2] -gt $c[0] -and $c[3] -gt $c[1] -and
        $c[0] -ge ($r[0] - 0.1) -and $c[1] -ge ($r[1] - 0.1) -and
        $c[2] -le ($r[2] + 0.1) -and $c[3] -le ($r[3] + 0.1)
}

function Invoke-NoteReplaceControlClick(
    [string]$Name,
    [string]$ControlName,
    [IntPtr]$ExpectedRoot,
    [string]$ExpectedPanelIdentity,
    [switch]$AllowDisabled
) {
    $state = Assert-NoteReplaceRoot $ExpectedRoot $ExpectedPanelIdentity
    $paint = Get-NoteReplaceCurrentPanel $state $ExpectedPanelIdentity
    $property = $paint.controls.PSObject.Properties[$ControlName]
    if ($null -eq $property -or !(Test-NoteReplaceRect $property.Value) -or
        (!$AllowDisabled -and !$property.Value.enabled)) {
        throw "NoteReplace $ControlName is not a current visible enabled response"
    }
    $capturedControl = $property.Value
    $capturedContentRevision = [long]$paint.content_revision
    $capturedQueryRevision = [long]$paint.query_revision
    $capturedNavigationRevision = [long]$paint.navigation_revision

    # Re-read the actual owner immediately before converting and clicking the
    # response. A valid rectangle from an older frame must not authorize input
    # after a query, selection, or layout change.
    $state = Assert-NoteReplaceRoot $ExpectedRoot $ExpectedPanelIdentity
    $paint = Get-NoteReplaceCurrentPanel $state $ExpectedPanelIdentity
    $property = $paint.controls.PSObject.Properties[$ControlName]
    if ($null -eq $property -or $null -eq $property.Value -or
        [string]$property.Value.response_id -cne [string]$capturedControl.response_id -or
        (!$AllowDisabled -and !$property.Value.enabled) -or
        [long]$paint.content_revision -ne $capturedContentRevision -or
        [long]$paint.query_revision -ne $capturedQueryRevision -or
        [long]$paint.navigation_revision -ne $capturedNavigationRevision -or
        (@($property.Value.rect) -join ',') -cne (@($capturedControl.rect) -join ',') -or
        (@($property.Value.clipped_rect) -join ',') -cne (@($capturedControl.clipped_rect) -join ',')) {
        throw "NoteReplace $ControlName response or panel revisions changed before native input"
    }
    $control = $property.Value
    if (!(Test-NoteReplaceRect $control)) { throw "NoteReplace $ControlName response became clipped before native input" }
    $ppp = [double]$state.pixels_per_point
    if ($ppp -le 0 -or [double]::IsNaN($ppp) -or [double]::IsInfinity($ppp)) {
        throw 'NoteReplace current response has an invalid root pixel scale'
    }
    $rect = $control.clipped_rect
    $clientX = [int][Math]::Round((([double]$rect[0] + [double]$rect[2]) / 2.0) * $ppp)
    $clientY = [int][Math]::Round((([double]$rect[1] + [double]$rect[3]) / 2.0) * $ppp)
    $screenX = 0; $screenY = 0
    if (![JumpModeNativeProbe]::TryClientToScreen($ExpectedRoot, $clientX, $clientY, [ref]$screenX, [ref]$screenY)) {
        throw "could not map the current NoteReplace $ControlName response to screen coordinates"
    }
    $hit = [JumpModeNativeProbe]::WindowAtScreenPoint($screenX, $screenY)
    if ($hit -eq [IntPtr]::Zero -or [int][JumpModeNativeProbe]::WindowProcessId($hit) -ne $processId -or
        [JumpModeNativeProbe]::RootWindow($hit) -ne $ExpectedRoot) {
        throw "NoteReplace $ControlName response hit test found a foreign or occluding window"
    }
    Assert-OwnedRepairRootForeground $ExpectedRoot
    if (![JumpModeNativeProbe]::ClickScreenPoint($screenX, $screenY)) {
        throw "SendInput click failed for current NoteReplace $ControlName response"
    }
    Write-Event 'note_replace_response_clicked' @{
        name = $Name
        control = $ControlName
        response_id = [string]$control.response_id
        logic_frame_id = [long]$state.logic_frame_id
        ui_pass_id = [long]$state.ui_pass_id
        content_revision = [long]$paint.content_revision
        query_revision = [long]$paint.query_revision
    }
    return $state
}

function Invoke-NoteReplaceTargetClick(
    [string]$Name,
    [object]$Observation,
    [string]$TargetId,
    [string]$ExpectedAction,
    [IntPtr]$ExpectedRoot
) {
    $state = Assert-NoteReplaceRoot $ExpectedRoot
    if ($null -eq $Observation -or [long]$state.logic_frame_id -lt [long]$Observation.logic_frame_id) {
        throw 'NoteReplace launcher target frame is stale before native input'
    }
    $targets = @($state.targets | Where-Object {
        $_.surface -ceq 'launcher_results' -and [string]$_.target_id -ceq $TargetId -and
            $_.eligible -and $_.root_action -and [string]$_.root_action.action -ceq $ExpectedAction
    })
    if ($targets.Count -ne 1) { throw 'NoteReplace exact launcher target is no longer a unique eligible current response' }
    $target = $targets[0]
    if (!(Test-NoteReplaceRect $target)) { throw 'NoteReplace launcher target is clipped or has invalid current geometry' }
    $ppp = [double]$state.pixels_per_point
    $rect = $target.clipped_rect
    $clientX = [int][Math]::Round((([double]$rect[0] + [double]$rect[2]) / 2.0) * $ppp)
    $clientY = [int][Math]::Round((([double]$rect[1] + [double]$rect[3]) / 2.0) * $ppp)
    $screenX = 0; $screenY = 0
    if (![JumpModeNativeProbe]::TryClientToScreen($ExpectedRoot, $clientX, $clientY, [ref]$screenX, [ref]$screenY)) {
        throw 'could not map the exact NoteReplace launcher result to screen coordinates'
    }
    $hit = [JumpModeNativeProbe]::WindowAtScreenPoint($screenX, $screenY)
    if ($hit -eq [IntPtr]::Zero -or [int][JumpModeNativeProbe]::WindowProcessId($hit) -ne $processId -or
        [JumpModeNativeProbe]::RootWindow($hit) -ne $ExpectedRoot) {
        throw 'NoteReplace launcher result hit test found a foreign or occluding window'
    }
    $null = Assert-NoteReplaceRoot $ExpectedRoot
    if (![JumpModeNativeProbe]::ClickScreenPoint($screenX, $screenY)) {
        throw 'SendInput click failed for the exact NoteReplace launcher result'
    }
    Write-Event 'note_replace_launcher_response_clicked' @{
        name = $Name
        target_id = $TargetId
        action_id = $ExpectedAction
        logic_frame_id = [long]$state.logic_frame_id
        ui_pass_id = [long]$state.ui_pass_id
    }
}

function Invoke-NoteReplaceRootControlClick(
    [string]$Name,
    [string]$ControlName,
    [IntPtr]$ExpectedRoot,
    [string]$ExpectedActionId = ''
) {
    $state = Assert-NoteReplaceRoot $ExpectedRoot
    $rootPaint = Get-NoteReplaceRootPaint $state
    if ($null -eq $rootPaint) { throw 'NoteReplace current ROOT response owner is unavailable' }
    $control = if ($ControlName -ceq 'query') { $rootPaint.query } elseif ($ControlName -ceq 'result') { $rootPaint.result } else { $null }
    if ($null -eq $control -or !(Test-NoteReplaceRect $control) -or !$control.enabled) {
        throw "NoteReplace ROOT $ControlName is not a current visible enabled Response"
    }
    if ($ControlName -ceq 'result' -and
        [string]$rootPaint.result_action_id -cne $ExpectedActionId) {
        throw 'NoteReplace ROOT result Response does not identify the exact generated note action'
    }
    $capturedId = [string]$control.response_id
    $capturedRect = @($control.rect)
    $capturedClip = @($control.clipped_rect)
    $capturedQuery = [string]$state.query
    $capturedGeneration = [long]$state.result_generation

    # Re-read a fresh pass and require the same actual response and result
    # generation before using its current clipped geometry.
    $state = Assert-NoteReplaceRoot $ExpectedRoot
    $rootPaint = Get-NoteReplaceRootPaint $state
    $control = if ($null -ne $rootPaint -and $ControlName -ceq 'query') {
        $rootPaint.query
    } elseif ($null -ne $rootPaint -and $ControlName -ceq 'result') {
        $rootPaint.result
    } else { $null }
    if ($null -eq $control -or !$control.enabled -or
        [string]$control.response_id -cne $capturedId -or
        (@($control.rect) -join ',') -cne ($capturedRect -join ',') -or
        (@($control.clipped_rect) -join ',') -cne ($capturedClip -join ',') -or
        [string]$state.query -cne $capturedQuery -or
        [long]$state.result_generation -ne $capturedGeneration -or
        ($ControlName -ceq 'result' -and [string]$rootPaint.result_action_id -cne $ExpectedActionId)) {
        throw "NoteReplace ROOT $ControlName response or query/results changed before native input"
    }
    if (!(Test-NoteReplaceRect $control)) { throw "NoteReplace ROOT $ControlName response became clipped" }
    $ppp = [double]$state.pixels_per_point
    if ($ppp -le 0 -or [double]::IsNaN($ppp) -or [double]::IsInfinity($ppp)) {
        throw 'NoteReplace ROOT response has an invalid pixel scale'
    }
    $rect = $control.clipped_rect
    $clientX = [int][Math]::Round((([double]$rect[0] + [double]$rect[2]) / 2.0) * $ppp)
    $clientY = [int][Math]::Round((([double]$rect[1] + [double]$rect[3]) / 2.0) * $ppp)
    $screenX = 0; $screenY = 0
    if (![JumpModeNativeProbe]::TryClientToScreen($ExpectedRoot, $clientX, $clientY, [ref]$screenX, [ref]$screenY)) {
        throw "could not map the current NoteReplace ROOT $ControlName response to screen coordinates"
    }
    $hit = [JumpModeNativeProbe]::WindowAtScreenPoint($screenX, $screenY)
    if ($hit -eq [IntPtr]::Zero -or [int][JumpModeNativeProbe]::WindowProcessId($hit) -ne $processId -or
        [JumpModeNativeProbe]::RootWindow($hit) -ne $ExpectedRoot) {
        throw "NoteReplace ROOT $ControlName response hit test found a foreign or occluding window"
    }
    Assert-OwnedRepairRootForeground $ExpectedRoot
    if (![JumpModeNativeProbe]::ClickScreenPoint($screenX, $screenY)) {
        throw "SendInput click failed for current NoteReplace ROOT $ControlName response"
    }
    Write-Event 'note_replace_root_response_clicked' @{
        name = $Name
        control = $ControlName
        response_id = $capturedId
        action_id = if ($ControlName -ceq 'result') { $ExpectedActionId } else { $null }
        logic_frame_id = [long]$state.logic_frame_id
        ui_pass_id = [long]$state.ui_pass_id
        result_generation = [long]$state.result_generation
    }
    return $state
}

function Wait-NoteReplacePanel([IntPtr]$ExpectedRoot, [long]$ExpectedActivation, [string]$Description) {
    return Wait-JumpModeObservation {
        param($state)
        $state.root_focused -and $state.note_replace.root_owner_allowed -and
            [long]$state.activation_count -eq $ExpectedActivation -and
            [string]$state.last_action.action_id -ceq 'note:open:jump-qa-note-001' -and
            $null -ne (Get-NoteReplaceCurrentPanel $state) -and
            $state.note_replace.dirty -eq $false -and
            $state.note_replace.view_mode -ceq 'edit'
    }.GetNewClosure() $CaptureTimeoutSeconds $Description
}

function Set-NoteReplaceOpenAction([IntPtr]$ExpectedRoot, [bool]$UseJump) {
    $query = "note open $script:noteReplaceExpectedTitle"
    $actionId = 'note:open:jump-qa-note-001'
    $activationBefore = [long](Read-JumpModeObservation).activation_count
    $baseline = Read-JumpModeObservation
    if ([string]$baseline.query -cne $query) {
        $rootPaint = Get-NoteReplaceRootPaint $baseline
        $queryResponse = $null
        if ($null -ne $rootPaint) { $queryResponse = $rootPaint.query }
        if ($null -eq $queryResponse -or !(Test-NoteReplaceRect $queryResponse) -or !$queryResponse.enabled) {
            throw 'NoteReplace could not identify one current eligible ROOT query TextEntry response'
        }
        $null = Invoke-NoteReplaceRootControlClick 'note-replace-focus-root-query' 'query' $ExpectedRoot
        $focused = Wait-JumpModeObservation {
            param($state)
            $rootPaint = Get-NoteReplaceRootPaint $state
            $state.root_focused -and $null -ne $rootPaint -and $rootPaint.query.focused -and
                [string]$state.query -ceq [string]$baseline.query -and
                [long]$state.activation_count -eq $activationBefore
        }.GetNewClosure() $CaptureTimeoutSeconds 'current ROOT query TextEntry focus for NoteReplace'
        $baseline = $focused
        if (![string]::IsNullOrEmpty([string]$baseline.query)) {
            Invoke-NoteReplaceKeyBatch 'note-replace-route-select-query' ([uint16[]]@(0x11, 0x41)) ([string[]]@('Ctrl', 'A')) $ExpectedRoot -FocusedControl 'query'
            Invoke-NoteReplaceKeyBatch 'note-replace-route-clear-query' ([uint16[]]@(0x08)) ([string[]]@('Backspace')) $ExpectedRoot -FocusedControl 'query'
        }
        Send-NoteReplaceUnicodeText 'note-replace-route-type-open-note' $query $ExpectedRoot -FocusedControl 'query'
    }
    $queryState = Wait-JumpModeObservation {
        param($state)
        $rootPaint = Get-NoteReplaceRootPaint $state
        $state.root_focused -and $null -ne $rootPaint -and $rootPaint.query.enabled -and
            $state.query_focused -and [string]$state.query -ceq $query -and
            [int]$state.result_count -eq 1 -and
            [long]$state.activation_count -eq $activationBefore -and
            [string]$rootPaint.result_action_id -ceq $actionId -and
            $null -ne $rootPaint.result -and $rootPaint.result.enabled -and
            (!$UseJump -or @($state.targets | Where-Object {
                $_.surface -ceq 'launcher_results' -and $_.eligible -and $_.root_action -and
                    [string]$_.root_action.action -ceq $actionId
            }).Count -eq 1)
    }.GetNewClosure() $CaptureTimeoutSeconds 'exact generated note opener result under the current ROOT query'
    if ($UseJump) {
        $target = @($queryState.targets | Where-Object {
            $_.surface -ceq 'launcher_results' -and $_.eligible -and $_.root_action -and
                [string]$_.root_action.action -ceq $actionId
        })[0]
        Invoke-NoteReplaceKeyBatch 'note-replace-open-hint-leader' ([uint16[]]@(0x10, 0x20)) ([string[]]@('Shift', 'Space')) $ExpectedRoot -FocusedControl 'query'
        $hinted = Wait-JumpModeObservation {
            param($state)
            $state.root_focused -and $state.root_owner_allowed -and $state.jump_mode_enabled -and
                $state.active_session -and [string]$state.query -ceq $query -and
                [long]$state.activation_count -eq $activationBefore -and
                @($state.targets | Where-Object {
                    $_.surface -ceq 'launcher_results' -and $_.eligible -and $_.root_action -and
                        [string]$_.root_action.action -ceq $actionId -and [string]$_.hint -match '^[A-Z]+$'
                }).Count -eq 1
        }.GetNewClosure() $CaptureTimeoutSeconds 'exact generated note opener in the active hint map'
        $hintTarget = @($hinted.targets | Where-Object {
            $_.surface -ceq 'launcher_results' -and $_.eligible -and $_.root_action -and
                [string]$_.root_action.action -ceq $actionId -and [string]$_.hint -match '^[A-Z]+$'
        })[0]
        foreach ($character in ([string]$hintTarget.hint).ToCharArray()) {
            Invoke-NoteReplaceKeyBatch 'note-replace-open-observed-hint' ([uint16[]]@([char]$character)) ([string[]]@([string]$character)) $ExpectedRoot
        }
        $selected = Wait-JumpModeObservation {
            param($state)
            !$state.active_session -and $state.root_focused -and $state.root_owner_allowed -and
                $state.query_focused -and [string]$state.query -ceq $query -and
                $null -ne $state.outcome -and $state.outcome.kind -ceq 'completed' -and
                [string]$state.outcome.target_id -ceq [string]$target.target_id -and
                [string]$state.selected_target_id -ceq [string]$target.target_id -and
                [string]$state.selected_action.action -ceq $actionId -and
                [long]$state.activation_count -eq $activationBefore
        }.GetNewClosure() $CaptureTimeoutSeconds 'focus-only Jump Mode selection of the exact generated note action'
        Invoke-NoteReplaceKeyBatch 'note-replace-open-fresh-enter' ([uint16[]]@(0x0D)) ([string[]]@('Enter')) $ExpectedRoot -FocusedControl 'query'
    } else {
        $null = Invoke-NoteReplaceRootControlClick 'note-replace-disabled-open-note' 'result' $ExpectedRoot $actionId
    }
    return Wait-NoteReplacePanel $ExpectedRoot ($activationBefore + 1) 'exact generated note panel after one current production open action'
}

function Invoke-NoteReplaceNativeWorkflow($StartupObservation) {
    if (!$script:noteReplaceFixtureScenario -or !$script:noteReplaceSourcePath -or !$script:noteReplaceOtherPath) {
        throw 'NoteReplace NativeOnly workflow has no validated generated notes fixture identity'
    }
    $root = Get-OwnedRootHandle
    if ($null -eq $StartupObservation -or [string]$StartupObservation.fixture_manifest_sha256 -cne $fixtureHash -or
        !$StartupObservation.root_focused -or
        $null -eq (Get-NoteReplaceRootPaint $StartupObservation)) {
        throw 'NoteReplace startup observation is not bound to the exact foreground ROOT fixture'
    }
    if (!(Test-RetainedProcessIdentity) -or [int][JumpModeNativeProbe]::WindowProcessId($root) -ne $processId -or
        $root -ne [IntPtr]([Convert]::ToInt64(([string]$script:appWindowMetadata.hwnd).Substring(2), 16))) {
        throw 'NoteReplace ROOT HWND/PID identity changed before the first native input'
    }
    $sourceDiskBefore = (Get-FileHash -Algorithm SHA256 -LiteralPath $script:noteReplaceSourcePath).Hash.ToLowerInvariant()
    $otherDiskBefore = (Get-FileHash -Algorithm SHA256 -LiteralPath $script:noteReplaceOtherPath).Hash.ToLowerInvariant()
    if ($sourceDiskBefore -cne $script:noteReplaceBeforeSha256 -or $otherDiskBefore -cne $script:noteReplaceOtherBeforeSha256) {
        throw 'generated NoteReplace source files changed after prebuilt fixture verification and before input'
    }
    $script:phase = 'note_replace_opening_generated_note'
    $opened = Set-NoteReplaceOpenAction $root ([bool]$script:noteReplaceJumpEnabled)
    $panelIdentity = [string]$opened.note_replace.panel_identity
    $activationBaseline = [long]$opened.activation_count
    Write-Event 'note_replace_panel_opened' @{
        fixture_scenario = $script:noteReplaceFixtureScenario
        jump_mode_enabled = [bool]$script:noteReplaceJumpEnabled
        panel_identity = $panelIdentity
        content_revision = [long]$opened.note_replace.content_revision
        activation_count = $activationBaseline
        root_hwnd = ('0x{0:X}' -f $root.ToInt64())
        source_disk_sha256 = $sourceDiskBefore
        unrelated_note_disk_sha256 = $otherDiskBefore
    }
    if (!$script:noteReplaceJumpEnabled) {
        $null = Invoke-NoteReplaceControlClick 'note-replace-disabled-open-find' 'find_toggle' $root $panelIdentity
        $findOpened = Wait-JumpModeObservation {
            param($state)
            $state.root_focused -and $state.note_replace.root_owner_allowed -and
                (Get-NoteReplaceCurrentPanel $state $panelIdentity) -and
                $state.note_replace.find_open -and $state.note_replace.controls.find_query -and
                $state.note_replace.controls.find_query.enabled
        }.GetNewClosure() $CaptureTimeoutSeconds 'Jump-disabled Note Replace Find query response'
        $null = Invoke-NoteReplaceControlClick 'note-replace-disabled-focus-query' 'find_query' $root $panelIdentity
        $focused = Wait-JumpModeObservation {
            param($state)
            (Get-NoteReplaceCurrentPanel $state $panelIdentity) -and
                $state.note_replace.controls.find_query -and $state.note_replace.controls.find_query.focused
        }.GetNewClosure() $CaptureTimeoutSeconds 'Jump-disabled current Find TextEntry focus'
        Send-NoteReplaceUnicodeText 'note-replace-disabled-search-literal' 'literal Ω $1\path' $root $panelIdentity 'find_query'
        $searched = Wait-JumpModeObservation {
            param($state)
            $paint = Get-NoteReplaceCurrentPanel $state $panelIdentity
            $null -ne $paint -and $paint.find_open -and $paint.match_count -eq 2 -and
                $paint.source_only_match -and [long]$state.activation_count -eq $activationBaseline
        }.GetNewClosure() $CaptureTimeoutSeconds 'Jump-disabled literal Find result and source-only visibility'
        $null = Invoke-NoteReplaceControlClick 'note-replace-disabled-close-find' 'find_close' $root $panelIdentity
        $closed = Wait-JumpModeObservation {
            param($state)
            $paint = Get-NoteReplaceCurrentPanel $state $panelIdentity
            $null -ne $paint -and !$paint.find_open -and !$paint.dirty -and
                [long]$state.activation_count -eq $activationBaseline
        }.GetNewClosure() $CaptureTimeoutSeconds 'Jump-disabled Find closes without changing the note'
        if ((Get-FileHash -Algorithm SHA256 -LiteralPath $script:noteReplaceSourcePath).Hash.ToLowerInvariant() -cne $sourceDiskBefore -or
            (Get-FileHash -Algorithm SHA256 -LiteralPath $script:noteReplaceOtherPath).Hash.ToLowerInvariant() -cne $otherDiskBefore) {
            throw 'Jump-disabled Find-only leg changed a generated note file'
        }
        Write-Event 'note_replace_jump_disabled_focus_passed' @{
            panel_identity = $panelIdentity
            find_match_count = [int]$searched.note_replace.match_count
            source_only_match = [bool]$searched.note_replace.source_only_match
            find_closed = !$closed.note_replace.find_open
            disk_unchanged = $true
            activation_count = [long]$closed.activation_count
        }
        return [ordered]@{
            fixture_scenario = $script:noteReplaceFixtureScenario
            jump_mode_enabled = $false
            panel_identity = $panelIdentity
            find_match_count = [int]$searched.note_replace.match_count
            source_only_match = [bool]$searched.note_replace.source_only_match
            disk_unchanged = $true
            activation_count = [long]$closed.activation_count
        }
    }

    $marker = ' pre-replacement unsaved Ω'
    $script:noteReplaceExpectedPreReplace = $script:noteReplaceSourceBefore + $marker
    $script:noteReplaceExpectedPreReplaceSha256 = Get-NoteReplaceTextSha256 $script:noteReplaceExpectedPreReplace
    $expectedPreReplaceSha256 = [string]$script:noteReplaceExpectedPreReplaceSha256
    $null = Invoke-NoteReplaceControlClick 'note-replace-focus-editor-before-find' 'note_editor' $root $panelIdentity
    $editorForMarker = Wait-JumpModeObservation {
        param($state)
        $paint = Get-NoteReplaceCurrentPanel $state $panelIdentity
        $null -ne $paint -and $paint.controls.note_editor -and $paint.controls.note_editor.focused
    }.GetNewClosure() $CaptureTimeoutSeconds 'actual note editor focus before adding the unsaved pre-replacement marker'
    Invoke-NoteReplaceKeyBatch 'note-replace-move-to-end-before-find' ([uint16[]]@(0x11, 0x23)) ([string[]]@('Ctrl', 'End')) $root $panelIdentity 'note_editor'
    Send-NoteReplaceUnicodeText 'note-replace-add-unsaved-pre-replacement-marker' $marker $root $panelIdentity 'note_editor'
    $preReplace = Wait-JumpModeObservation {
        param($state)
        $paint = Get-NoteReplaceCurrentPanel $state $panelIdentity
        $null -ne $paint -and $paint.dirty -and
            [string]$paint.content_sha256 -ceq $expectedPreReplaceSha256 -and
            [long]$state.activation_count -eq $activationBaseline
    }.GetNewClosure() $CaptureTimeoutSeconds 'unsaved pre-replacement marker in the exact current note source'
    if ((Get-FileHash -Algorithm SHA256 -LiteralPath $script:noteReplaceSourcePath).Hash.ToLowerInvariant() -cne $sourceDiskBefore -or
        (Get-FileHash -Algorithm SHA256 -LiteralPath $script:noteReplaceOtherPath).Hash.ToLowerInvariant() -cne $otherDiskBefore) {
        throw 'pre-replacement unsaved marker changed a generated note file on disk'
    }
    $script:noteReplaceExpectedSourceAfterAll = $script:noteReplaceExpectedPreReplace.Replace(
        'literal Ω $1\path', [string]$script:noteReplaceExpectedReplacement, [StringComparison]::Ordinal
    )
    $script:noteReplaceExpectedAfterReplaceSha256 = Get-NoteReplaceTextSha256 $script:noteReplaceExpectedSourceAfterAll
    $expectedAfterReplaceSha256 = [string]$script:noteReplaceExpectedAfterReplaceSha256
    Write-Event 'note_replace_unsaved_marker_added' @{
        panel_identity = $panelIdentity
        content_revision = [long]$preReplace.note_replace.content_revision
        source_sha256 = [string]$preReplace.note_replace.content_sha256
        disk_unchanged = $true
    }

    $script:phase = 'note_replace_current_find'
    $null = Invoke-NoteReplaceControlClick 'note-replace-open-find' 'find_toggle' $root $panelIdentity
    $findOpen = Wait-JumpModeObservation {
        param($state)
        $paint = Get-NoteReplaceCurrentPanel $state $panelIdentity
        $null -ne $paint -and $paint.find_open -and $paint.controls.find_query -and
            $paint.controls.find_query.enabled
    }.GetNewClosure() $CaptureTimeoutSeconds 'Note Replace Find strip current query response'
    $null = Invoke-NoteReplaceControlClick 'note-replace-open-replace-expander' 'replace_expander' $root $panelIdentity
    $expanded = Wait-JumpModeObservation {
        param($state)
        $paint = Get-NoteReplaceCurrentPanel $state $panelIdentity
        $null -ne $paint -and $paint.find_open -and $paint.controls.replacement_input -and
            $paint.controls.replace_all
    }.GetNewClosure() $CaptureTimeoutSeconds 'expanded current literal replacement field and Replace All response'
    $null = Invoke-NoteReplaceControlClick 'note-replace-focus-original-query' 'find_query' $root $panelIdentity
    $queryFocused = Wait-JumpModeObservation {
        param($state)
        $paint = Get-NoteReplaceCurrentPanel $state $panelIdentity
        $null -ne $paint -and $paint.controls.find_query -and $paint.controls.find_query.focused
    }.GetNewClosure() $CaptureTimeoutSeconds 'actual Find TextEntry focus before typing generated literal query'
    Send-NoteReplaceUnicodeText 'note-replace-type-original-query' 'literal Ω $1\path' $root $panelIdentity 'find_query'
    $matches = Wait-JumpModeObservation {
        param($state)
        $paint = Get-NoteReplaceCurrentPanel $state $panelIdentity
        $null -ne $paint -and $paint.find_open -and $paint.match_count -eq 2 -and
            $paint.source_only_match -and [long]$state.activation_count -eq $activationBaseline
    }.GetNewClosure() $CaptureTimeoutSeconds 'two exact literal source-only Unicode matches in current Markdown'
    $null = Invoke-NoteReplaceControlClick 'note-replace-focus-replacement-input' 'replacement_input' $root $panelIdentity
    $replacementFocused = Wait-JumpModeObservation {
        param($state)
        $paint = Get-NoteReplaceCurrentPanel $state $panelIdentity
        $null -ne $paint -and $paint.controls.replacement_input -and $paint.controls.replacement_input.focused
    }.GetNewClosure() $CaptureTimeoutSeconds 'actual replacement TextEntry focus'
    Send-NoteReplaceUnicodeText 'note-replace-type-literal-replacement' ([string]$script:noteReplaceExpectedReplacement) $root $panelIdentity 'replacement_input'
    $readyToReplace = Wait-JumpModeObservation {
        param($state)
        $paint = Get-NoteReplaceCurrentPanel $state $panelIdentity
        $null -ne $paint -and $paint.match_count -eq 2 -and $paint.controls.replace_all -and
            $paint.controls.replace_all.enabled -and [long]$state.activation_count -eq $activationBaseline
    }.GetNewClosure() $CaptureTimeoutSeconds 'enabled current Replace All Response for the two source matches'
    $preReplaceRevision = [long]$readyToReplace.note_replace.content_revision
    $null = Invoke-NoteReplaceControlClick 'note-replace-all-original-matches' 'replace_all' $root $panelIdentity
    $replaced = Wait-JumpModeObservation {
        param($state)
        $paint = Get-NoteReplaceCurrentPanel $state $panelIdentity
        $null -ne $paint -and $paint.dirty -and $paint.content_revision -gt $preReplaceRevision -and
            $paint.match_count -eq 0 -and
            [string]$paint.content_sha256 -ceq $expectedAfterReplaceSha256 -and
            [long]$state.activation_count -eq $activationBaseline
    }.GetNewClosure() $CaptureTimeoutSeconds 'Replace All mutates the current unsaved note exactly once'
    $afterReplaceDisk = (Get-FileHash -Algorithm SHA256 -LiteralPath $script:noteReplaceSourcePath).Hash.ToLowerInvariant()
    if ($afterReplaceDisk -cne $sourceDiskBefore -or
        (Get-FileHash -Algorithm SHA256 -LiteralPath $script:noteReplaceOtherPath).Hash.ToLowerInvariant() -cne $otherDiskBefore) {
        throw 'Note Replace All wrote to disk before normal Save or changed the unrelated generated note'
    }
    $null = Invoke-NoteReplaceControlClick 'note-replace-focus-new-query' 'find_query' $root $panelIdentity
    $focusedNewQuery = Wait-JumpModeObservation {
        param($state)
        $paint = Get-NoteReplaceCurrentPanel $state $panelIdentity
        $null -ne $paint -and $paint.controls.find_query -and $paint.controls.find_query.focused
    }.GetNewClosure() $CaptureTimeoutSeconds 'current Find response focus for source-only replacement verification'
    Invoke-NoteReplaceKeyBatch 'note-replace-select-original-search' ([uint16[]]@(0x11, 0x41)) ([string[]]@('Ctrl', 'A')) $root $panelIdentity 'find_query'
    Send-NoteReplaceUnicodeText 'note-replace-search-inserted-literal' ([string]$script:noteReplaceExpectedReplacement) $root $panelIdentity 'find_query'
    $replacementSearch = Wait-JumpModeObservation {
        param($state)
        $paint = Get-NoteReplaceCurrentPanel $state $panelIdentity
        $null -ne $paint -and $paint.find_open -and $paint.match_count -eq 2 -and
            $paint.dirty -and [long]$state.activation_count -eq $activationBaseline
    }.GetNewClosure() $CaptureTimeoutSeconds 'literal replacement query finds both inserted occurrences in Markdown source'
    $null = Invoke-NoteReplaceControlClick 'note-replace-switch-to-preview-for-source-only-reveal' 'render' $root $panelIdentity
    $sourceOnly = Wait-JumpModeObservation {
        param($state)
        $paint = Get-NoteReplaceCurrentPanel $state $panelIdentity
        $null -ne $paint -and $paint.view_mode -ceq 'preview' -and $paint.find_open -and
            $paint.match_count -eq 2 -and $paint.source_only_match -and $paint.dirty -and
            [long]$state.activation_count -eq $activationBaseline
    }.GetNewClosure() $CaptureTimeoutSeconds 'Preview shows source-only replacement matches before Reveal in Edit'
    $null = Invoke-NoteReplaceControlClick 'note-replace-reveal-source-match' 'reveal_in_edit' $root $panelIdentity
    $revealed = Wait-JumpModeObservation {
        param($state)
        $paint = Get-NoteReplaceCurrentPanel $state $panelIdentity
        $null -ne $paint -and $paint.view_mode -ceq 'edit' -and $paint.match_count -eq 2 -and
            $null -ne $paint.selected_match_byte_range -and [long]$state.activation_count -eq $activationBaseline
    }.GetNewClosure() $CaptureTimeoutSeconds 'source-only Reveal selects an exact replacement occurrence in Edit'
    $null = Invoke-NoteReplaceControlClick 'note-replace-close-find-before-editor-undo' 'find_close' $root $panelIdentity
    $editorReady = Wait-JumpModeObservation {
        param($state)
        $paint = Get-NoteReplaceCurrentPanel $state $panelIdentity
        $null -ne $paint -and !$paint.find_open -and $paint.controls.note_editor -and
            $paint.controls.note_editor.enabled -and $paint.controls.note_editor.focused
    }.GetNewClosure() $CaptureTimeoutSeconds 'note editor current TextEdit focus for replacement Undo'
    $replacedRevision = [long]$editorReady.note_replace.content_revision
    Invoke-NoteReplaceKeyBatch 'note-replace-editor-undo' ([uint16[]]@(0x11, 0x5A)) ([string[]]@('Ctrl', 'Z')) $root $panelIdentity 'note_editor'
    $undone = Wait-JumpModeObservation {
        param($state)
        $paint = Get-NoteReplaceCurrentPanel $state $panelIdentity
        $null -ne $paint -and $paint.dirty -and $paint.content_revision -gt $replacedRevision -and
            $paint.match_count -eq 0 -and
            [string]$paint.content_sha256 -ceq $expectedPreReplaceSha256 -and
            [long]$state.activation_count -eq $activationBaseline
    }.GetNewClosure() $CaptureTimeoutSeconds 'one editor-owned Undo restores the exact pre-replacement unsaved source'
    $undoRevision = [long]$undone.note_replace.content_revision
    Invoke-NoteReplaceKeyBatch 'note-replace-editor-redo' ([uint16[]]@(0x11, 0x59)) ([string[]]@('Ctrl', 'Y')) $root $panelIdentity 'note_editor'
    $redone = Wait-JumpModeObservation {
        param($state)
        $paint = Get-NoteReplaceCurrentPanel $state $panelIdentity
        $null -ne $paint -and $paint.dirty -and $paint.content_revision -gt $undoRevision -and
            $paint.match_count -eq 2 -and
            [string]$paint.content_sha256 -ceq $expectedAfterReplaceSha256 -and
            [long]$state.activation_count -eq $activationBaseline
    }.GetNewClosure() $CaptureTimeoutSeconds 'one editor-owned Redo restores the unsaved replacement'
    $null = Invoke-NoteReplaceControlClick 'note-replace-normal-save' 'save' $root $panelIdentity
    $saved = Wait-JumpModeObservation {
        param($state)
        $paint = Get-NoteReplaceCurrentPanel $state $panelIdentity
        $null -ne $paint -and !$paint.dirty -and $paint.content_revision -ge $redone.note_replace.content_revision -and
            [long]$state.activation_count -eq $activationBaseline
    }.GetNewClosure() $CaptureTimeoutSeconds 'normal note Save persists the current literal replacement'
    $savedSource = [IO.File]::ReadAllText($script:noteReplaceSourcePath, [Text.UTF8Encoding]::new($false, $true))
    if ($savedSource -cne $script:noteReplaceExpectedSourceAfterAll) {
        throw 'normal Save did not persist the exact literal Replace All output while preserving surrounding CRLF bytes'
    }
    $script:noteReplaceSavedSha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $script:noteReplaceSourcePath).Hash.ToLowerInvariant()
    if ((Get-FileHash -Algorithm SHA256 -LiteralPath $script:noteReplaceOtherPath).Hash.ToLowerInvariant() -cne $otherDiskBefore) {
        throw 'normal Save changed the unrelated generated note'
    }

    $null = Invoke-NoteReplaceControlClick 'note-replace-reopen-find-for-discard-check' 'find_toggle' $root $panelIdentity
    $findAgain = Wait-JumpModeObservation {
        param($state)
        $paint = Get-NoteReplaceCurrentPanel $state $panelIdentity
        $null -ne $paint -and $paint.find_open -and $paint.controls.find_close
    }.GetNewClosure() $CaptureTimeoutSeconds 'Find strip before testing close/discard'
    $null = Invoke-NoteReplaceControlClick 'note-replace-close-find-for-discard-check' 'find_close' $root $panelIdentity
    $closedFind = Wait-JumpModeObservation {
        param($state)
        $paint = Get-NoteReplaceCurrentPanel $state $panelIdentity
        $null -ne $paint -and !$paint.find_open -and $paint.controls.note_editor -and $paint.controls.note_editor.enabled
    }.GetNewClosure() $CaptureTimeoutSeconds 'Find strip closed before creating a post-save unsaved edit'
    $null = Invoke-NoteReplaceControlClick 'note-replace-focus-editor-for-discard-check' 'note_editor' $root $panelIdentity
    $editorFocused = Wait-JumpModeObservation {
        param($state)
        $paint = Get-NoteReplaceCurrentPanel $state $panelIdentity
        $null -ne $paint -and $paint.controls.note_editor -and $paint.controls.note_editor.focused
    }.GetNewClosure() $CaptureTimeoutSeconds 'actual note editor focus before a separate unsaved edit'
    Invoke-NoteReplaceKeyBatch 'note-replace-move-to-end-for-discard-check' ([uint16[]]@(0x11, 0x23)) ([string[]]@('Ctrl', 'End')) $root $panelIdentity 'note_editor'
    Invoke-NoteReplaceKeyBatch 'note-replace-add-newline-for-discard-check' ([uint16[]]@(0x0D)) ([string[]]@('Enter')) $root $panelIdentity 'note_editor'
    Send-NoteReplaceUnicodeText 'note-replace-add-discard-only-edit' 'unsaved discard marker' $root $panelIdentity 'note_editor'
    $postSaveDirty = Wait-JumpModeObservation {
        param($state)
        $paint = Get-NoteReplaceCurrentPanel $state $panelIdentity
        $null -ne $paint -and $paint.dirty -and [long]$state.activation_count -eq $activationBaseline
    }.GetNewClosure() $CaptureTimeoutSeconds 'separate post-save edit marks only the current note dirty'
    if ((Get-FileHash -Algorithm SHA256 -LiteralPath $script:noteReplaceSourcePath).Hash.ToLowerInvariant() -cne $script:noteReplaceSavedSha256 -or
        (Get-FileHash -Algorithm SHA256 -LiteralPath $script:noteReplaceOtherPath).Hash.ToLowerInvariant() -cne $otherDiskBefore) {
        throw 'post-save unsaved edit changed one of the generated note files before discard'
    }
    Invoke-NoteReplaceKeyBatch 'note-replace-close-dirty-note-with-production-escape' ([uint16[]]@(0x1B)) ([string[]]@('Escape')) $root $panelIdentity 'note_editor'
    $discardPrompt = Wait-JumpModeObservation {
        param($state)
        $paint = Get-NoteReplaceCurrentPanel $state $panelIdentity
        $null -ne $paint -and $paint.discard_prompt -and $paint.dirty -and
            $paint.controls.discard_changes -and $paint.controls.discard_changes.enabled -and
            [long]$state.activation_count -eq $activationBaseline
    }.GetNewClosure() $CaptureTimeoutSeconds 'actual NotePanel close opens its unsaved discard prompt'
    $null = Invoke-NoteReplaceControlClick 'note-replace-confirm-discard' 'discard_changes' $root $panelIdentity
    $noteReplaceSlug = [string]$script:noteReplaceSlug
    $discarded = Wait-JumpModeObservation {
        param($state)
        $state.root_focused -and $state.note_replace_root.root_owner_allowed -and
            @($state.opened_note_slugs | Where-Object { [string]$_ -ceq $noteReplaceSlug }).Count -eq 0 -and
            [long]$state.activation_count -eq $activationBaseline
    }.GetNewClosure() $CaptureTimeoutSeconds 'actual Discard Changes closes the generated dirty note panel'
    $sourceAfterDiscard = (Get-FileHash -Algorithm SHA256 -LiteralPath $script:noteReplaceSourcePath).Hash.ToLowerInvariant()
    $otherAfterDiscard = (Get-FileHash -Algorithm SHA256 -LiteralPath $script:noteReplaceOtherPath).Hash.ToLowerInvariant()
    if ($sourceAfterDiscard -cne $script:noteReplaceSavedSha256 -or $otherAfterDiscard -cne $otherDiskBefore) {
        throw 'Discard Changes altered the last saved current note or the unrelated note'
    }
    Write-Event 'note_replace_native_workflow_passed' @{
        fixture_scenario = $script:noteReplaceFixtureScenario
        jump_mode_enabled = $true
        panel_identity = $panelIdentity
        activation_count = [long]$discarded.activation_count
        original_match_count = [int]$matches.note_replace.match_count
        replaced_match_count = [int]$sourceOnly.note_replace.match_count
        source_only_match = [bool]$sourceOnly.note_replace.source_only_match
        undo_restored_saved_baseline = !$undone.note_replace.dirty
        redo_restored_replacement = [bool]$redone.note_replace.dirty
        saved_source_sha256 = $script:noteReplaceSavedSha256
        unrelated_note_sha256 = $otherAfterDiscard
        discard_prompt_observed = [bool]$discardPrompt.note_replace.discard_prompt
        discard_preserved_saved_file = $true
    }
    return [ordered]@{
        fixture_scenario = $script:noteReplaceFixtureScenario
        jump_mode_enabled = $true
        panel_identity = $panelIdentity
        activation_count = [long]$discarded.activation_count
        original_match_count = [int]$matches.note_replace.match_count
        replaced_match_count = [int]$sourceOnly.note_replace.match_count
        source_only_match = [bool]$sourceOnly.note_replace.source_only_match
        undo_restored_saved_baseline = !$undone.note_replace.dirty
        redo_restored_replacement = [bool]$redone.note_replace.dirty
        saved_source_sha256 = $script:noteReplaceSavedSha256
        unrelated_note_sha256 = $otherAfterDiscard
        discard_prompt_observed = [bool]$discardPrompt.note_replace.discard_prompt
        discard_preserved_saved_file = $true
    }
}

function Invoke-TargetRepairNativeWorkflow($StartupObservation) {
    if ($null -eq $script:targetRepairFixtureManifest -or
        !$script:targetRepairExpectedActionPath -or !$script:targetRepairExpectedFolderPath) {
        throw 'TargetRepair NativeOnly workflow has no validated generated fixture identity'
    }
    $root = Get-OwnedRootHandle
    $startupHwnd = ('0x{0:X}' -f $root.ToInt64())
    if ($startupHwnd -cne [string]$script:appWindowMetadata.hwnd -or
        [int][JumpModeNativeProbe]::WindowProcessId($root) -ne $processId) {
        throw 'TargetRepair root HWND no longer matches the pre-input owned window metadata'
    }
    if ($null -eq $StartupObservation -or [string]$StartupObservation.fixture_manifest_sha256 -cne $fixtureHash) {
        throw 'TargetRepair startup observation is not bound to the validated generated fixture'
    }
    $script:targetRepairPrimaryActivationBaseline = [long]$StartupObservation.activation_count
    $script:ownedAppBaselineTree = @(Get-OwnedTreeSnapshot)
    if (@($script:ownedAppBaselineTree | Where-Object { [int]$_.pid -eq $processId }).Count -ne 1) {
        throw 'TargetRepair did not capture its exact owned process tree before any user input'
    }
    $script:targetRepairActionStoreBeforeSha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $script:targetRepairActionStorePath).Hash.ToLowerInvariant()
    $script:targetRepairFolderStoreBeforeSha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $script:targetRepairFolderStorePath).Hash.ToLowerInvariant()

    $fixture = $script:targetRepairFixtureManifest
    $actionOriginal = [string]$script:targetRepairOriginalActionPath
    $folderOriginal = [string]$script:targetRepairOriginalFolderPath
    $initialActions = @(Get-Content -Raw -LiteralPath $script:targetRepairActionStorePath | ConvertFrom-Json)
    $initialFolders = @(Get-Content -Raw -LiteralPath $script:targetRepairFolderStorePath | ConvertFrom-Json)
    if ($initialActions.Count -ne 1 -or
        [string]$initialActions[0].label -cne [string]$fixture.action_label -or
        [string]$initialActions[0].desc -cne [string]$fixture.action_description -or
        [string]$initialActions[0].action -cne $actionOriginal -or
        [string]$initialActions[0].args -cne [string]$fixture.action_args -or
        $initialFolders.Count -ne 1 -or
        [string]$initialFolders[0].label -cne [string]$fixture.folder_label -or
        [string]$initialFolders[0].path -cne $folderOriginal -or
        [string]$initialFolders[0].alias -cne [string]$fixture.folder_alias) {
        throw 'TargetRepair isolated stores do not match the exact generated records before native input'
    }
    Assert-TargetRepairAssociations $actionOriginal $folderOriginal

    Write-Summary 'BLOCKED' 'The owned ROOT is ready. Waiting for the generated missing custom action and saved folder through actual current Jump Mode hints and native picker ownership.'
    $actionQuery = "app $([string]$fixture.action_label)"
    $actionResult = Set-TargetRepairQuery $root $actionQuery ([string]$fixture.action_label)
    Write-Event 'target_repair_action_query_ready' @{
        query = $actionQuery
        target_id = @($actionResult.targets | Where-Object { $_.surface -ceq 'launcher_results' -and $_.eligible -and $_.label -ceq [string]$fixture.action_label })[0].target_id
        result_generation = [long]$actionResult.result_generation
        activation_count = [long]$actionResult.activation_count
    }
    $dialog = Open-TargetRepairActionSheet $root $actionQuery ([string]$fixture.action_label) 'custom_action' 'custom_action.repair_target'
    if ([string]$dialog.target_repair.target_owner -cne 'custom_action' -or
        [int]$dialog.target_repair.record_index -ne 0 -or
        [string]$dialog.target_repair.replacement_kind -cne 'file') {
        throw 'TargetRepair custom action dialog is not the exact first saved custom File target'
    }
    $fileType = @($dialog.target_repair.controls | Where-Object { $_.name -ceq 'file_type' -and $_.enabled -and $_.painted })
    if ($fileType.Count -ne 1) { throw 'TargetRepair custom action did not expose its current explicit file/application type Response' }
    $null = Invoke-TargetRepairRootClick 'target-repair-select-file-type' $dialog $fileType[0] $root ([long]$dialog.target_repair.request_token)
    $dialog = Wait-TargetRepairDialog 'custom_action' 'file'
    $dialog = Invoke-TargetRepairBrowseAndChoose 'target-repair-action-file' 'custom_action' 'file' $script:targetRepairExpectedActionPath $root -CancelFirst
    $confirm = @($dialog.target_repair.controls | Where-Object { $_.name -ceq 'confirm' -and $_.enabled -and $_.painted })
    if ($confirm.Count -ne 1) { throw 'TargetRepair custom action has no current enabled explicit confirm Response' }
    $actionToken = [long]$dialog.target_repair.request_token
    $actionRevision = [long]$dialog.target_repair.owner_revision
    $null = Invoke-TargetRepairRootClick 'target-repair-confirm-action-file' $dialog $confirm[0] $root $actionToken
    $actionSubmitted = Wait-JumpModeObservation {
        param($state)
        [long]$state.activation_count -eq $script:targetRepairPrimaryActivationBaseline -and
            $state.app_editor_open -and
            ($null -eq $state.target_repair -or
                ([long]$state.target_repair.request_token -eq $actionToken -and
                    [long]$state.target_repair.owner_revision -eq $actionRevision -and
                    $state.target_repair.pending))
    } ([Math]::Min($CaptureTimeoutSeconds, 20)) 'custom action repair pending or fast child completion'
    if ($null -ne $actionSubmitted.target_repair -and
        ([string]$actionSubmitted.target_repair.target_owner -cne 'custom_action' -or
            [long]$actionSubmitted.target_repair.request_token -ne $actionToken -or
            [long]$actionSubmitted.target_repair.owner_revision -ne $actionRevision)) {
        throw 'TargetRepair action submission did not retain its exact child-dialog identity'
    }
    $actionDone = Complete-TargetRepairChildAndCloseParentEditor 'target-repair-action' $actionToken $actionRevision $script:targetRepairPrimaryActivationBaseline $root
    Assert-TargetRepairActionStore $script:targetRepairExpectedActionPath
    Assert-TargetRepairAssociations $script:targetRepairExpectedActionPath $folderOriginal
    if ((Get-FileHash -Algorithm SHA256 -LiteralPath $script:targetRepairFolderStorePath).Hash.ToLowerInvariant() -cne $script:targetRepairFolderStoreBeforeSha256) {
        throw 'custom action repair unexpectedly changed folders.json'
    }
    $actionActivationDelta = [long]$actionDone.activation_count - $script:targetRepairPrimaryActivationBaseline
    if ($actionActivationDelta -ne 0) { throw 'TargetRepair custom action repair changed the primary action activation count' }
    Write-Event 'target_repair_action_committed' @{
        request_token = $actionToken
        owner_revision = $actionRevision
        exact_action_metadata_and_args_preserved = $true
        replacement_verified_in_store = $true
        unrelated_folder_store_unchanged = $true
        activation_count = [long]$actionDone.activation_count
        primary_activation_count_before = $script:targetRepairPrimaryActivationBaseline
        primary_activation_count_delta = $actionActivationDelta
        primary_activation_count_unchanged = $true
    }

    $folderQuery = "f $([string]$fixture.folder_alias)"
    $folderResult = Set-TargetRepairQuery $root $folderQuery ([string]$fixture.folder_alias)
    Write-Event 'target_repair_folder_query_ready' @{
        query = $folderQuery
        target_id = @($folderResult.targets | Where-Object { $_.surface -ceq 'launcher_results' -and $_.eligible -and $_.label -ceq [string]$fixture.folder_alias })[0].target_id
        result_generation = [long]$folderResult.result_generation
        activation_count = [long]$folderResult.activation_count
    }
    $dialog = Open-TargetRepairActionSheet $root $folderQuery ([string]$fixture.folder_alias) 'folder' 'folder.repair_target'
    if ([string]$dialog.target_repair.target_owner -cne 'folder' -or
        [int]$dialog.target_repair.record_index -ne 0 -or
        [string]$dialog.target_repair.replacement_kind -cne 'folder') {
        throw 'TargetRepair folder dialog is not the exact saved FolderEntry target'
    }
    $dialog = Invoke-TargetRepairBrowseAndChoose 'target-repair-folder' 'folder' 'folder' $script:targetRepairExpectedFolderPath $root
    $confirm = @($dialog.target_repair.controls | Where-Object { $_.name -ceq 'confirm' -and $_.enabled -and $_.painted })
    if ($confirm.Count -ne 1) { throw 'TargetRepair folder has no current enabled explicit confirm Response' }
    $folderToken = [long]$dialog.target_repair.request_token
    $folderRevision = [long]$dialog.target_repair.owner_revision
    $null = Invoke-TargetRepairRootClick 'target-repair-confirm-folder' $dialog $confirm[0] $root $folderToken
    $folderSubmitted = Wait-JumpModeObservation {
        param($state)
        [long]$state.activation_count -eq $script:targetRepairPrimaryActivationBaseline -and
            $state.app_editor_open -and
            ($null -eq $state.target_repair -or
                ([long]$state.target_repair.request_token -eq $folderToken -and
                    [long]$state.target_repair.owner_revision -eq $folderRevision -and
                    $state.target_repair.pending))
    } ([Math]::Min($CaptureTimeoutSeconds, 20)) 'saved folder repair pending or fast child completion'
    if ($null -ne $folderSubmitted.target_repair -and
        ([string]$folderSubmitted.target_repair.target_owner -cne 'folder' -or
            [long]$folderSubmitted.target_repair.request_token -ne $folderToken -or
            [long]$folderSubmitted.target_repair.owner_revision -ne $folderRevision)) {
        throw 'TargetRepair folder submission did not retain its exact child-dialog identity'
    }
    $folderDone = Complete-TargetRepairChildAndCloseParentEditor 'target-repair-folder' $folderToken $folderRevision $script:targetRepairPrimaryActivationBaseline $root
    Assert-TargetRepairActionStore $script:targetRepairExpectedActionPath
    Assert-TargetRepairFolderStore $script:targetRepairExpectedFolderPath
    Assert-TargetRepairAssociations $script:targetRepairExpectedActionPath $script:targetRepairExpectedFolderPath
    $newTree = @(Get-OwnedTreeSnapshot)
    $baselineIdentities = [Collections.Generic.HashSet[string]]::new()
    foreach ($item in $script:ownedAppBaselineTree) { $null = $baselineIdentities.Add("$($item.pid):$($item.creation_time_utc)") }
    $newDescendants = @($newTree | Where-Object { !$baselineIdentities.Contains("$($_.pid):$($_.creation_time_utc)") })
    if ($newDescendants.Count -ne 0) { throw 'TargetRepair started a new child process instead of only updating the generated stores' }
    $primaryActivationDelta = [long]$folderDone.activation_count - $script:targetRepairPrimaryActivationBaseline
    if ($primaryActivationDelta -ne 0 -or $newDescendants.Count -ne 0) {
        throw 'TargetRepair changed the primary action activation count or created an owned child process'
    }
    Write-Event 'target_repair_folder_committed' @{
        request_token = $folderToken
        owner_revision = $folderRevision
        action_store_exact = $true
        folder_label_alias_and_replacement_exact = $true
        action_folder_ui_intents_only = $true
        primary_activation_count_before = $script:targetRepairPrimaryActivationBaseline
        primary_activation_count_after = [long]$folderDone.activation_count
        primary_activation_count_delta = $primaryActivationDelta
        execution_count_derived_from_primary_activation_delta = $primaryActivationDelta
        child_process_count_unchanged = $true
        activation_count = [long]$folderDone.activation_count
    }
    return [ordered]@{
        execution_count_derived_from_primary_activation_delta = $primaryActivationDelta
        primary_activation_count_before = $script:targetRepairPrimaryActivationBaseline
        primary_activation_count_after = [long]$folderDone.activation_count
        owned_child_process_count = $newDescendants.Count
        exact_associations_verified = $true
    }
}

function Get-SettingsSearchSourceDiffSha256 {
    $previousOutputEncoding = [Console]::OutputEncoding
    try {
        [Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
        $diffLines = @(& git -C $repoRoot diff --binary HEAD -- src Cargo.toml Cargo.lock dev/jump_mode/fixture_generator)
        $diffExitCode = $LASTEXITCODE
    }
    finally {
        [Console]::OutputEncoding = $previousOutputEncoding
    }
    if ($diffExitCode -ne 0) { throw 'could not calculate tracked build-input source diff for the prebuilt manifest' }
    $diffText = if ($diffLines.Count -gt 0) { [string]::Join("`n", [string[]]$diffLines) + "`n" } else { '' }
    $sha = [Security.Cryptography.SHA256]::Create()
    try { return [Convert]::ToHexString($sha.ComputeHash([Text.UTF8Encoding]::new($false).GetBytes($diffText))).ToLowerInvariant() }
    finally { $sha.Dispose() }
}

function Get-PrebuiltAbsolutePath([object]$PathValue, [string]$FieldName) {
    $value = [string]$PathValue
    if ([string]::IsNullOrWhiteSpace($value) -or ![IO.Path]::IsPathRooted($value)) {
        throw "prebuilt manifest $FieldName must be an absolute path"
    }
    return [IO.Path]::GetFullPath($value)
}

function Get-TargetRepairProfileRelativePath([string]$ProfilePath, [string]$RelativePath) {
    $normalizedRelative = $RelativePath.Replace('/', [IO.Path]::DirectorySeparatorChar).Replace('\', [IO.Path]::DirectorySeparatorChar)
    return [IO.Path]::GetFullPath((Join-Path $ProfilePath $normalizedRelative))
}

function ConvertTo-TargetRepairPathIdentity([string]$PathValue) {
    $normalized = [IO.Path]::GetFullPath($PathValue.Replace('/', [IO.Path]::DirectorySeparatorChar).Replace('\', [IO.Path]::DirectorySeparatorChar))
    if ($normalized.StartsWith('\\?\UNC\', [StringComparison]::OrdinalIgnoreCase)) {
        return '\\' + $normalized.Substring(8)
    }
    if ($normalized.StartsWith('\\?\', [StringComparison]::OrdinalIgnoreCase)) {
        return $normalized.Substring(4)
    }
    return $normalized
}

function Test-TargetRepairPathIdentity([string]$ActualPath, [string]$ExpectedPath) {
    return [string]::Equals(
        (ConvertTo-TargetRepairPathIdentity $ActualPath),
        (ConvertTo-TargetRepairPathIdentity $ExpectedPath),
        [StringComparison]::OrdinalIgnoreCase)
}

function Invoke-PrebuiltNativeScenario {
    if ($Scenario -notin @('SettingsSearch', 'TargetRepair', 'NoteReplace') -or !$NativeOnly -or $FixturesOnly) {
        throw 'prebuilt native-only mode is valid only for SettingsSearch, TargetRepair, or NoteReplace'
    }
    if ([string]::IsNullOrWhiteSpace($PrebuiltArtifactManifest)) { throw 'NativeOnly requires PrebuiltArtifactManifest' }
    $allowedFixtureScenarios = switch ($Scenario) {
        'SettingsSearch' { @('phase2-settings') }
        'TargetRepair' { @('target-repair') }
        'NoteReplace' { @('note-replace-enabled', 'note-replace-disabled') }
    }
    $artifactManifestPath = [IO.Path]::GetFullPath($PrebuiltArtifactManifest)
    if (!(Test-Path -LiteralPath $artifactManifestPath -PathType Leaf)) { throw 'prebuilt artifact manifest does not exist' }
    $artifact = Get-Content -Raw -LiteralPath $artifactManifestPath | ConvertFrom-Json
    $expectedFixtureScenario = [string]$artifact.fixture_scenario
    if ($artifact.schema_version -ne 1 -or $expectedFixtureScenario -notin $allowedFixtureScenarios) {
        throw "prebuilt artifact manifest must be schema 1 for one of the isolated $($allowedFixtureScenarios -join ', ') fixtures"
    }
    $branch = (& git -C $repoRoot branch --show-current).Trim()
    if ($LASTEXITCODE -ne 0) { throw 'could not read the current source branch for prebuilt artifact validation' }
    $headSha = (& git -C $repoRoot rev-parse HEAD).Trim()
    if ($LASTEXITCODE -ne 0) { throw 'could not read the current source HEAD for prebuilt artifact validation' }
    if ($branch -cne [string]$artifact.source_branch -or $branch -cne 'feat/productivity-tools' -or
        $headSha -cne [string]$artifact.source_head) {
        throw 'prebuilt artifact branch or HEAD does not match the current productivity-tools source'
    }
    $sourceDiffSha = Get-SettingsSearchSourceDiffSha256
    if ($sourceDiffSha -cne ([string]$artifact.source_diff_sha256).ToLowerInvariant()) {
        throw 'prebuilt artifact source diff does not match tracked source/Cargo/fixture inputs'
    }
    if ([string]$artifact.cargo_profile -cne 'fast-dev' -or
        (@($artifact.cargo_features | ForEach-Object { [string]$_ } | Sort-Object) -join ',') -cne 'dev-smoke-binaries,qa-framebuffer-capture') {
        throw 'prebuilt artifact does not identify the required fast-dev QA feature build'
    }
    if ($Scenario -eq 'NoteReplace') {
        $runnerPath = Get-PrebuiltAbsolutePath $artifact.runner_path 'runner_path'
        $currentRunnerPath = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot 'run.ps1'))
        if (![string]::Equals($runnerPath, $currentRunnerPath, [StringComparison]::OrdinalIgnoreCase) -or
            [string]::IsNullOrWhiteSpace([string]$artifact.runner_sha256) -or
            (Get-FileHash -Algorithm SHA256 -LiteralPath $currentRunnerPath).Hash.ToLowerInvariant() -cne [string]$artifact.runner_sha256) {
            throw 'NoteReplace prebuilt manifest does not bind the current native runner path and bytes'
        }
    }

    $receiptPath = Get-PrebuiltAbsolutePath $artifact.build_receipt_path 'build_receipt_path'
    $appPath = Get-PrebuiltAbsolutePath $artifact.app_path 'app_path'
    $fixtureExePath = Get-PrebuiltAbsolutePath $artifact.fixture_exe_path 'fixture_exe_path'
    $profilePath = Get-PrebuiltAbsolutePath $artifact.fixture_profile_path 'fixture_profile_path'
    $fixtureManifestPath = Get-PrebuiltAbsolutePath $artifact.fixture_manifest_path 'fixture_manifest_path'
    if (!(Test-Path -LiteralPath $receiptPath -PathType Leaf) -or
        !(Test-Path -LiteralPath $appPath -PathType Leaf) -or
        !(Test-Path -LiteralPath $fixtureExePath -PathType Leaf) -or
        !(Test-Path -LiteralPath $profilePath -PathType Container) -or
        !(Test-Path -LiteralPath $fixtureManifestPath -PathType Leaf)) {
        throw 'prebuilt artifact is missing its build receipt, executable, isolated profile, or fixture manifest'
    }
    $expectedManifestPath = [IO.Path]::GetFullPath((Join-Path $profilePath 'fixture_manifest.json'))
    if (![string]::Equals($fixtureManifestPath, $expectedManifestPath, [StringComparison]::OrdinalIgnoreCase)) {
        throw 'prebuilt fixture manifest is not the manifest at the isolated profile root'
    }
    if ([IO.Path]::GetFileName($appPath) -cne 'multi_launcher.exe' -or
        [IO.Path]::GetFileName($fixtureExePath) -cne 'jump_mode_fixture.exe' -or
        ![string]::Equals([IO.Path]::GetDirectoryName($appPath), [IO.Path]::GetDirectoryName($fixtureExePath), [StringComparison]::OrdinalIgnoreCase)) {
        throw 'prebuilt app and fixture executables must be the expected fast-dev artifacts from one output directory'
    }
    $receiptHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $receiptPath).Hash.ToLowerInvariant()
    $appHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $appPath).Hash.ToLowerInvariant()
    $fixtureExeHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $fixtureExePath).Hash.ToLowerInvariant()
    $fixtureManifestHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $fixtureManifestPath).Hash.ToLowerInvariant()
    foreach ($item in @(
        @{ Field = 'build_receipt_sha256'; Actual = $receiptHash },
        @{ Field = 'app_sha256'; Actual = $appHash },
        @{ Field = 'fixture_exe_sha256'; Actual = $fixtureExeHash },
        @{ Field = 'fixture_manifest_sha256'; Actual = $fixtureManifestHash }
    )) {
        if ([string]$artifact.($item.Field) -cne $item.Actual) { throw "prebuilt artifact $($item.Field) does not match its file" }
    }
    $buildReceipt = Get-Content -Raw -LiteralPath $receiptPath | ConvertFrom-Json
    if ([string]$buildReceipt.Reason -cne 'completed' -or [int]$buildReceipt.ExitCode -ne 0 -or [bool]$buildReceipt.RootAlive) {
        throw 'prebuilt build receipt does not prove a completed zero-exit build with its root gone'
    }
    $fixtureManifest = Get-Content -Raw -LiteralPath $fixtureManifestPath | ConvertFrom-Json
    if ($Scenario -eq 'SettingsSearch') {
        if ([string]$fixtureManifest.scenario -cne 'phase2-settings' -or
            [string]$fixtureManifest.phase2_surface -cne 'settings' -or
            !$fixtureManifest.experimental_jump_mode_enabled -or
            @($fixtureManifest.enabled_plugins | ForEach-Object { [string]$_ } | Sort-Object) -join ',' -cne 'notes,settings' -or
            [int]$fixtureManifest.action_count -ne 0 -or
            !$fixtureManifest.safety.profile_isolated_by_working_directory -or
            !$fixtureManifest.safety.notes_path_supplied_by_ml_notes_dir -or
            $fixtureManifest.safety.clipboard_plugin_enabled -or
            $fixtureManifest.safety.clipboard_modify_plugin_enabled -or
            $fixtureManifest.safety.macros_enabled -or $fixtureManifest.safety.network_plugin_enabled -or
            $fixtureManifest.safety.custom_actions_present -or
            $fixtureManifest.safety.paste_next_hotkey_enabled -or
            $fixtureManifest.safety.paste_cancel_hotkey_enabled) {
            throw 'prebuilt fixture manifest does not prove the isolated, Jump-enabled, no-action Settings profile'
        }
    } elseif ($Scenario -eq 'TargetRepair') {
        if ([string]$fixtureManifest.scenario -cne 'target-repair' -or
            $null -ne $fixtureManifest.phase2_surface -or
            !$fixtureManifest.experimental_jump_mode_enabled -or
            @($fixtureManifest.enabled_plugins | ForEach-Object { [string]$_ } | Sort-Object) -join ',' -cne 'folders,missing,notes' -or
            [int]$fixtureManifest.action_count -ne 1 -or
            !$fixtureManifest.safety.profile_isolated_by_working_directory -or
            !$fixtureManifest.safety.notes_path_supplied_by_ml_notes_dir -or
            $fixtureManifest.safety.clipboard_plugin_enabled -or
            $fixtureManifest.safety.clipboard_modify_plugin_enabled -or
            $fixtureManifest.safety.macros_enabled -or $fixtureManifest.safety.network_plugin_enabled -or
            !$fixtureManifest.safety.custom_actions_present -or
            $fixtureManifest.safety.paste_next_hotkey_enabled -or
            $fixtureManifest.safety.paste_cancel_hotkey_enabled) {
            throw 'prebuilt fixture manifest does not prove the isolated, Jump-enabled TargetRepair profile'
        }
        $repair = $fixtureManifest.target_repair
        if ($null -eq $repair -or
            [string]::IsNullOrWhiteSpace([string]$repair.action_label) -or
            [string]::IsNullOrWhiteSpace([string]$repair.action_description) -or
            [string]::IsNullOrWhiteSpace([string]$repair.action_args) -or
            [string]::IsNullOrWhiteSpace([string]$repair.folder_label) -or
            [string]::IsNullOrWhiteSpace([string]$repair.folder_alias)) {
            throw 'TargetRepair fixture manifest is missing its exact inert action/folder records'
        }
        $associations = $repair.associations
        if ($null -eq $associations -or
            [string]::IsNullOrWhiteSpace([string]$associations.action_favorite_label) -or
            [string]::IsNullOrWhiteSpace([string]$associations.folder_favorite_label) -or
            [string]::IsNullOrWhiteSpace([string]$associations.unrelated_favorite_label) -or
            [string]::IsNullOrWhiteSpace([string]$associations.unrelated_path) -or
            [IO.Path]::IsPathRooted([string]$associations.unrelated_path) -or
            [string]::IsNullOrWhiteSpace([string]$associations.action_pin_query) -or
            [string]::IsNullOrWhiteSpace([string]$associations.folder_pin_query) -or
            [string]::IsNullOrWhiteSpace([string]$associations.unrelated_pin_query) -or
            [uint32]$associations.action_usage_count -eq 0 -or
            [uint32]$associations.folder_usage_count -eq 0 -or
            [uint32]$associations.unrelated_usage_count -eq 0) {
            throw 'TargetRepair fixture manifest is missing its exact inert favorite, pin, and usage associations'
        }
        foreach ($relative in @($repair.missing_action_path, $repair.missing_folder_path)) {
            if ([string]::IsNullOrWhiteSpace([string]$relative) -or [IO.Path]::IsPathRooted([string]$relative)) {
                throw 'TargetRepair missing source paths must be nonempty profile-relative paths'
            }
            if (Test-Path -LiteralPath (Join-Path $profilePath ([string]$relative))) {
                throw 'TargetRepair fixture unexpectedly contains a path it claims is missing'
            }
        }
        $script:targetRepairExpectedActionPath = [IO.Path]::GetFullPath((Join-Path $profilePath ([string]$repair.replacement_file)))
        $script:targetRepairExpectedFolderPath = [IO.Path]::GetFullPath((Join-Path $profilePath ([string]$repair.replacement_folder)))
        if (!(Test-Path -LiteralPath $script:targetRepairExpectedActionPath -PathType Leaf) -or
            !(Test-Path -LiteralPath $script:targetRepairExpectedFolderPath -PathType Container) -or
            !(Test-Path -LiteralPath (Join-Path $script:targetRepairExpectedFolderPath 'fixture.txt') -PathType Leaf)) {
            throw 'TargetRepair replacement file/folder is not present inside the isolated profile'
        }
        $script:targetRepairFixtureManifest = $repair
        $script:targetRepairActionStorePath = Join-Path $profilePath 'actions.json'
        $script:targetRepairFolderStorePath = Join-Path $profilePath 'folders.json'
        $script:targetRepairFavoriteStorePath = Join-Path $profilePath 'fav.json'
        $script:targetRepairPinStorePath = Join-Path $profilePath 'history_pins.json'
        $script:targetRepairUsageStorePath = Join-Path $profilePath 'usage.json'
        if (!(Test-Path -LiteralPath $script:targetRepairActionStorePath -PathType Leaf) -or
            !(Test-Path -LiteralPath $script:targetRepairFolderStorePath -PathType Leaf) -or
            !(Test-Path -LiteralPath $script:targetRepairFavoriteStorePath -PathType Leaf) -or
            !(Test-Path -LiteralPath $script:targetRepairPinStorePath -PathType Leaf) -or
            !(Test-Path -LiteralPath $script:targetRepairUsageStorePath -PathType Leaf)) {
            throw 'TargetRepair isolated action, folder, favorite, pin, or usage stores are missing'
        }
        $actionRecords = @(Get-Content -Raw -LiteralPath $script:targetRepairActionStorePath | ConvertFrom-Json)
        $folderRecords = @(Get-Content -Raw -LiteralPath $script:targetRepairFolderStorePath | ConvertFrom-Json)
        $expectedActionSource = Get-TargetRepairProfileRelativePath $profilePath ([string]$repair.missing_action_path)
        $expectedFolderSource = Get-TargetRepairProfileRelativePath $profilePath ([string]$repair.missing_folder_path)
        if ($actionRecords.Count -ne 1 -or
            [string]$actionRecords[0].label -cne [string]$repair.action_label -or
            [string]$actionRecords[0].desc -cne [string]$repair.action_description -or
            !(Test-TargetRepairPathIdentity ([string]$actionRecords[0].action) $expectedActionSource) -or
            [string]$actionRecords[0].args -cne [string]$repair.action_args -or
            $folderRecords.Count -ne 1 -or
            [string]$folderRecords[0].label -cne [string]$repair.folder_label -or
            !(Test-TargetRepairPathIdentity ([string]$folderRecords[0].path) $expectedFolderSource) -or
            [string]$folderRecords[0].alias -cne [string]$repair.folder_alias) {
            throw 'TargetRepair persisted source stores do not match the exact generated inert records'
        }
        $script:targetRepairOriginalActionPath = [string]$actionRecords[0].action
        $script:targetRepairOriginalFolderPath = [string]$folderRecords[0].path
        $expectedUnrelatedPath = Get-TargetRepairProfileRelativePath $profilePath ([string]$repair.associations.unrelated_path)
        $favoriteRecords = @(Get-Content -Raw -LiteralPath $script:targetRepairFavoriteStorePath | ConvertFrom-Json)
        if ($favoriteRecords.Count -ne 3 -or
            !(Test-TargetRepairPathIdentity ([string]$favoriteRecords[2].action) $expectedUnrelatedPath)) {
            throw 'TargetRepair unrelated association does not match its exact generated profile-relative identity'
        }
        $script:targetRepairUnrelatedPath = [string]$favoriteRecords[2].action
        Assert-TargetRepairAssociations $script:targetRepairOriginalActionPath $script:targetRepairOriginalFolderPath
    } elseif ($Scenario -eq 'NoteReplace') {
        $noteReplaceScenarios = @('note-replace-enabled', 'note-replace-disabled')
        if ([string]$fixtureManifest.scenario -notin $noteReplaceScenarios -or
            [string]$fixtureManifest.scenario -cne $expectedFixtureScenario -or
            $null -ne $fixtureManifest.phase2_surface -or
            [int]$fixtureManifest.action_count -ne 0 -or
            [bool]$fixtureManifest.experimental_jump_mode_enabled -ne ($expectedFixtureScenario -ceq 'note-replace-enabled') -or
            @($fixtureManifest.enabled_plugins | ForEach-Object { [string]$_ } | Sort-Object) -join ',' -cne 'notes' -or
            !$fixtureManifest.safety.profile_isolated_by_working_directory -or
            !$fixtureManifest.safety.notes_path_supplied_by_ml_notes_dir -or
            $fixtureManifest.safety.clipboard_plugin_enabled -or
            $fixtureManifest.safety.clipboard_modify_plugin_enabled -or
            $fixtureManifest.safety.macros_enabled -or $fixtureManifest.safety.network_plugin_enabled -or
            $fixtureManifest.safety.custom_actions_present -or
            $fixtureManifest.safety.paste_next_hotkey_enabled -or
            $fixtureManifest.safety.paste_cancel_hotkey_enabled -or
            [string]$fixtureManifest.external_note_open -cne 'Neither' -or
            $fixtureManifest.safety.notes_path_supplied_by_ml_notes_dir -ne $true) {
            throw 'prebuilt fixture manifest does not prove the exact isolated NoteReplace Jump Mode profile'
        }
        if ($null -eq $fixtureManifest.note_ids -or
            @($fixtureManifest.note_ids | ForEach-Object { [string]$_ }) -notcontains 'jump-qa-note-001' -or
            @($fixtureManifest.note_ids | ForEach-Object { [string]$_ }) -notcontains 'jump-qa-note-002') {
            throw 'NoteReplace fixture does not contain the exact two generated note identities'
        }
        $notesDirectory = Join-Path $profilePath 'notes'
        $script:noteReplaceSourcePath = Join-Path $notesDirectory 'jump-qa-note-001.md'
        $script:noteReplaceOtherPath = Join-Path $notesDirectory 'jump-qa-note-002.md'
        if (!(Test-Path -LiteralPath $script:noteReplaceSourcePath -PathType Leaf) -or
            !(Test-Path -LiteralPath $script:noteReplaceOtherPath -PathType Leaf)) {
            throw 'NoteReplace generated note files are missing from the isolated ML_NOTES_DIR profile'
        }
        $oldLiteral = 'literal Ω $1\path'
        $newLiteral = 'edited Ω $cash\value'
        $script:noteReplaceExpectedTitle = 'Jump QA Replace Note'
        $expectedSource = "# $script:noteReplaceExpectedTitle" + "`r`n`r`n" +
            'Tag: #jump_qa_bucket_01' + "`r`n`r`n" +
            '[shown Ω](<literal Ω $1\path>)' + "`r`n" +
            '[shown Ω](<literal Ω $1\path>)' + "`r`n" +
            'keep this line' + "`r`n"
        $actualSource = [IO.File]::ReadAllText($script:noteReplaceSourcePath, [Text.UTF8Encoding]::new($false, $true))
        if ($actualSource -cne $expectedSource) {
            throw 'NoteReplace current note source does not match the exact generated Unicode/CRLF/literal fixture'
        }
        $script:noteReplaceSourceBefore = $actualSource
        $script:noteReplaceOtherBeforeSha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $script:noteReplaceOtherPath).Hash.ToLowerInvariant()
        $script:noteReplaceFixtureScenario = $expectedFixtureScenario
        $script:noteReplaceJumpEnabled = [bool]$fixtureManifest.experimental_jump_mode_enabled
        $script:noteReplaceExpectedSourceAfterAll = $actualSource.Replace($oldLiteral, $newLiteral, [StringComparison]::Ordinal)
        $script:noteReplaceExpectedReplacement = $newLiteral
        $script:noteReplaceSlug = 'jump-qa-note-001'
        $script:noteReplaceBeforeSha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $script:noteReplaceSourcePath).Hash.ToLowerInvariant()
        $script:noteReplaceSavedSha256 = $null
    } else {
        throw "prebuilt native scenario $Scenario has no fixture validator"
    }
    foreach ($directory in @('notes', 'templates', 'screenshot-output')) {
        if (!(Test-Path -LiteralPath (Join-Path $profilePath $directory) -PathType Container)) {
            throw "isolated fixture profile is missing its $directory directory"
        }
    }
    $settingsPath = Join-Path $profilePath 'settings.json'
    if (!(Test-Path -LiteralPath $settingsPath -PathType Leaf)) { throw 'isolated fixture settings.json is missing' }
    $persistedSettings = Get-Content -Raw -LiteralPath $settingsPath | ConvertFrom-Json
    if ($Scenario -eq 'NoteReplace' -and [string]$persistedSettings.note.default_view_mode -cne 'edit') {
        throw 'NoteReplace fixture must use its explicitly generated Edit default view'
    }
    if ($null -ne $persistedSettings.paste_sequence.next_hotkey -or
        $null -ne $persistedSettings.paste_sequence.cancel_hotkey -or
        $persistedSettings.screenshot_save_file -or $persistedSettings.screenshot_auto_save -or
        $persistedSettings.screenshot_use_editor) {
        throw 'isolated profile has active Paste Sequence shortcuts or screenshot side effects'
    }
    $script:prebuiltSettingsPath = [IO.Path]::GetFullPath($settingsPath)
    $script:prebuiltSettingsSha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $script:prebuiltSettingsPath).Hash.ToLowerInvariant()
    $script:headSha = $headSha
    $script:sourceBranch = $branch
    $script:sourceDiffSha256 = $sourceDiffSha
    $script:fixtureHash = $fixtureManifestHash
    $script:executablePath = $appPath
    $script:executableHash = $appHash
    $script:targetDirectory = [IO.Path]::GetDirectoryName([IO.Path]::GetDirectoryName($appPath))
    $script:profileRoot = $profilePath
    $script:buildReceiptIdentity = "prebuilt-fast-dev;features=$cargoFeatures;head=$headSha;exe_sha256=$appHash;build_receipt_sha256=$receiptHash"
    $script:fixtureScenarioStatus = 'PASS'
    $script:fixtureScenarioReason = "The externally produced app/fixture hashes, completed build receipt, current source identity and isolated $expectedFixtureScenario profile were validated before launch."
    $script:fixtureScenarioArgument = $expectedFixtureScenario
    $script:phase = 'prebuilt_artifacts_validated'
    Write-Event 'prebuilt_native_artifacts_validated' @{
        source_head = $headSha
        source_branch = $branch
        source_diff_sha256 = $sourceDiffSha
        app_sha256 = $appHash
        fixture_exe_sha256 = $fixtureExeHash
        fixture_manifest_sha256 = $fixtureManifestHash
        build_receipt_sha256 = $receiptHash
        fixture_scenario = $fixtureManifest.scenario
        paste_next_disabled = $true
        paste_cancel_disabled = $true
    }
    Update-Scenarios
    Test-Phase2WorkflowHelpers

    if (!$interactive -or $sessionId -eq 0) {
        $script:scenarioStatus = 'NOT_RUN_ENVIRONMENT'
        $script:scenarioReason = 'Prebuilt artifacts validated, but Windows reports no interactive user session; no app or native input was started.'
        $script:runStatus = 'NOT_RUN_ENVIRONMENT'
        $script:phase = 'prebuilt_native_not_run_environment'
        Update-Scenarios
        return
    }
    if (Test-Path -LiteralPath $captureRoot) { throw 'QA framebuffer output leaf must be absent before app launch' }
    $qaRequestPath = Join-Path $evidenceRoot 'qa-framebuffer-request.json'
    $qaRequest = [ordered]@{
        run_id = $runId
        request_id = switch ($Scenario) {
            'TargetRepair' { 'target-repair-native' }
            'NoteReplace' { 'note-replace-native' }
            default { 'settings-search-native' }
        }
        target = 'ROOT'
        trigger = 'f11'
        capture_count = if ($Scenario -in @('TargetRepair', 'NoteReplace')) { 1 } else { $phase2CaptureCount }
        diagnostics = $false
        output_dir = $captureRoot
        fixture_manifest_sha256 = $fixtureHash
        build_receipt_identity = $buildReceiptIdentity
        jump_mode_observation = [ordered]@{
            wait_for_active_hints = $false
            phase2 = if ($Scenario -in @('TargetRepair', 'NoteReplace')) { $null } else { [ordered]@{ surface = 'settings'; use_empty_paste_invocation_context = $false } }
            settings_search = if ($Scenario -in @('TargetRepair', 'NoteReplace')) { $null } else { [ordered]@{ expected_query = 'fuzzy weight' } }
            target_repair = if ($Scenario -eq 'TargetRepair') { [ordered]@{
                expected_action_replacement = $script:targetRepairExpectedActionPath
                expected_folder_replacement = $script:targetRepairExpectedFolderPath
            } } else { $null }
            note_replace = if ($Scenario -eq 'NoteReplace') { [ordered]@{
                expected_slug = $script:noteReplaceSlug
            } } else { $null }
        }
    }
    Write-Json $qaRequestPath $qaRequest
    Initialize-NativeQueryProbe
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
    $script:process = [Diagnostics.Process]::new()
    $process.StartInfo = $psi
    if (!$process.Start()) { throw 'failed to start the validated prebuilt Multi Launcher app' }
    $script:processId = $process.Id
    $script:processStartUtc = $process.StartTime.ToUniversalTime()
    $script:processSessionId = $process.SessionId
    $script:stdoutTask = $process.StandardOutput.ReadToEndAsync()
    $script:stderrTask = $process.StandardError.ReadToEndAsync()
    Write-Event 'owned_app_started' @{
        pid = $processId
        start_time_utc = $processStartUtc.ToString('O')
        session_id = $processSessionId
        executable = $executablePath
        executable_sha256 = $executableHash
        working_directory = $profileRoot
        fixture_manifest_sha256 = $fixtureHash
        build_receipt_identity = $buildReceiptIdentity
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
    if (!$treeObserved) { throw 'could not record the prebuilt app process identity in the Windows process table' }
    $script:phase = switch ($Scenario) {
        'TargetRepair' { 'target_repair_waiting_for_input' }
        'NoteReplace' { 'note_replace_waiting_for_input' }
        default { 'settings_search_waiting_for_input' }
    }
    $script:scenarioReason = switch ($Scenario) {
        'TargetRepair' { 'Validated prebuilt artifacts are running in the isolated profile; waiting for the exact owned ROOT query and repair-response observations before bounded native input.' }
        'NoteReplace' { 'Validated prebuilt artifacts are running in the isolated profile; waiting for the exact current note-panel and replacement-response observations before bounded native input.' }
        default { 'Validated prebuilt artifacts are running in the isolated profile; waiting for exact owned ROOT/query/Settings response observations before bounded native input.' }
    }
    Write-Event 'owned_process_tree_observed' @{ processes = $ownedTreeSnapshot }
    Write-Json $script:manifestPath ([ordered]@{
        schema_version = 1
        run_id = $runId
        milestone = $runMilestone
        status = $runStatus
        phase = $phase
        started_at_utc = $startedAt.ToString('O')
        repository_root = $repoRoot
        source_head = $headSha
        source_branch = $branch
        source_diff_sha256 = $sourceDiffSha
        interactive_session = $interactive
        session_id = $sessionId
        overall_timeout_seconds = $OverallTimeoutSeconds
        fixture_profile = $profileRoot
        fixture_manifest_sha256 = $fixtureHash
        executable = [ordered]@{ path = $executablePath; sha256 = $executableHash; pid = $processId; started_at_utc = $processStartUtc.ToString('O') }
        fixture_exe_path = $fixtureExePath
        fixture_exe_sha256 = $fixtureExeHash
        build_receipt_path = $receiptPath
        build_receipt_sha256 = $receiptHash
        build_receipt_identity = $buildReceiptIdentity
        capture = if ($Scenario -eq 'TargetRepair') {
            [ordered]@{ request = 'qa-framebuffer-request.json'; output_dir = 'capture'; source = 'current-pass ROOT response observations for TargetRepair controls; no desktop image capture requested'; count = 0; scenario = 'TargetRepair'; surface = $null }
        } elseif ($Scenario -eq 'NoteReplace') {
            [ordered]@{ request = 'qa-framebuffer-request.json'; output_dir = 'capture'; source = 'payload-free current-pass ROOT NoteReplace response observations; no framebuffer image capture requested'; count = 0; scenario = 'NoteReplace'; surface = $null }
        } else {
            [ordered]@{ request = 'qa-framebuffer-request.json'; output_dir = 'capture'; source = 'three native ROOT/F11 captures bound to current Settings Search QA observations'; count = $phase2CaptureCount; scenario = 'SettingsSearch'; surface = 'Settings' }
        }
        cleanup = $null
    })
    $startupObservation = Wait-SettingsSearchStartupReadiness
    if (!(Test-RetainedProcessIdentity)) { throw 'retained app PID/birth identity changed after Settings Search startup readiness' }
    $process.Refresh()
    if ($process.HasExited) {
        $script:processExitCode = $process.ExitCode
        throw "isolated app exited after Settings Search startup readiness (exit $script:processExitCode)"
    }
    $readyWindow = $process.MainWindowHandle
    if ($readyWindow -eq [IntPtr]::Zero) { throw 'owned main HWND disappeared after ROOT query readiness' }
    $readyWindowProcessId = [int][JumpModeNativeProbe]::WindowProcessId($readyWindow)
    if ($readyWindowProcessId -ne $processId) { throw "ready main HWND belongs to PID $readyWindowProcessId instead of retained PID $processId" }
    $readyHwnd = ('0x{0:X}' -f $readyWindow.ToInt64())
    $script:appWindowMetadata = Get-OwnedWindowMetadata
    if ($script:appWindowMetadata.hwnd -cne $readyHwnd -or
        [int]$script:appWindowMetadata.process_id -ne $processId) {
        throw 'owned main-window identity changed between ROOT query readiness and metadata capture'
    }
    Write-Event 'prebuilt_native_startup_ready' @{
        process_id = $processId
        process_started_at_utc = $processStartUtc.ToString('O')
        hwnd = $readyHwnd
        logic_frame_id = $startupObservation.logic_frame_id
        ui_pass_id = $startupObservation.ui_pass_id
        query_response_id = [string](@($startupObservation.targets | Where-Object {
            $_.surface -ceq 'launcher_results' -and $_.is_query_input -and $_.kind -ceq 'text_entry'
        })[0].target_id)
        query_response = 'current ROOT launcher query TextEntry observed; no foreground activation or input was sent'
    }
    Write-Event 'owned_app_window_metadata' @{ os_version = [Environment]::OSVersion.VersionString; architecture = [Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString(); session_id = $processSessionId; window = $appWindowMetadata }
    if (!(Test-Path -LiteralPath $prebuiltSettingsPath -PathType Leaf)) { throw 'isolated settings.json disappeared before Settings Search input' }
    $script:prebuiltSettingsPreInputSha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $prebuiltSettingsPath).Hash.ToLowerInvariant()
    Write-Event $(switch ($Scenario) {
        'TargetRepair' { 'target_repair_settings_file_before_first_input' }
        'NoteReplace' { 'note_replace_settings_file_before_first_input' }
        default { 'settings_search_settings_file_before_first_input' }
    }) @{
        before_launch_sha256 = $prebuiltSettingsSha256
        before_first_input_sha256 = $script:prebuiltSettingsPreInputSha256
        app_pid = $processId
        app_started_at_utc = $processStartUtc.ToString('O')
    }
    if ($Scenario -eq 'TargetRepair') {
        Write-Summary 'BLOCKED' 'The validated prebuilt app is waiting for exact owned missing-result and native-picker observations. NativeOnly does not build binaries or generate a fixture.'
        $targetRepairResult = Invoke-TargetRepairNativeWorkflow $startupObservation
        $script:scenarioStatus = 'PASS'
        $script:scenarioReason = 'The generated custom action and saved folder were repaired through their actual action-sheet and owned native picker controls; exact favorite/pin/usage associations persisted, and the primary activation delta was zero.'
        $script:runStatus = 'PASS'
        $script:phase = 'target_repair_complete'
        Update-Scenarios
        Write-Event 'target_repair_native_passed' @{
            action_target_replaced = $true
            folder_target_replaced = $true
            cancelled_file_picker_preserved_old_path = $true
            action_metadata_args_preserved = $true
            folder_alias_preserved = $true
            execution_count_derived_from_primary_activation_delta = $targetRepairResult.execution_count_derived_from_primary_activation_delta
            primary_activation_count_before = $targetRepairResult.primary_activation_count_before
            primary_activation_count_after = $targetRepairResult.primary_activation_count_after
            owned_child_process_count = $targetRepairResult.owned_child_process_count
            exact_associations_verified = $targetRepairResult.exact_associations_verified
            process_id = $processId
            process_started_at_utc = $processStartUtc.ToString('O')
        }
        Write-Summary 'PASS' "Both generated saved targets were repaired through the current application UI and same-process owned native picker dialogs. The file-picker cancel left its draft unchanged; exact action/folder metadata and favorite/pin/usage associations were preserved. Primary activation delta: $($targetRepairResult.execution_count_derived_from_primary_activation_delta); new owned child processes: $($targetRepairResult.owned_child_process_count)."
        return
    }
    if ($Scenario -eq 'NoteReplace') {
        Write-Summary 'BLOCKED' 'The validated prebuilt app is waiting for exact current ROOT query and Note Replace response observations. No note body or replacement text is included in the QA observation.'
        $noteReplaceResult = Invoke-NoteReplaceNativeWorkflow $startupObservation
        $script:scenarioStatus = 'PASS'
        $script:scenarioReason = 'The generated current Markdown note was opened through the actual note command; literal Replace All, source-only Reveal, same-owner Undo/Redo, normal Save, and subsequent discard preserved the second generated note.'
        if (!$script:noteReplaceJumpEnabled) {
            $script:scenarioReason = 'The Jump-disabled profile accepted normal current query/editor input and opened the exact generated note without a Jump Mode hint session; this was the short focus-only leg.'
        }
        $script:runStatus = 'PASS'
        $script:phase = 'note_replace_complete'
        Update-Scenarios
        Write-Event 'note_replace_native_passed' $noteReplaceResult
        Write-Summary 'PASS' $script:scenarioReason
        return
    }
    Write-Summary 'BLOCKED' 'The validated prebuilt app is waiting for its exact native Settings Search route. No build or fixture generator is run in NativeOnly mode.'
    Invoke-SettingsSearchNativeWorkflow
    $script:scenarioStatus = 'PASS'
    $script:scenarioReason = 'The exact settings:search query and two real Settings controls were activated through fresh Jump Mode input; slider/numeric edits stayed draft-only and Cancel preserved effective settings. Native Jump-off control interaction was not attempted; that branch is covered by deterministic real-response tests only.'
    $script:runStatus = 'PASS'
    $script:phase = 'settings_search_complete'
    Update-Scenarios
    Write-Event 'settings_search_native_passed' @{
        captures = @($phase2Captures | ForEach-Object { $_.capture_index })
        root_activation_count = $phase2ActivationCount
        slider_response_id = $phase2Captures[0].settings_search.highlight.response_id
        numeric_response_id = $phase2Captures[2].settings_search.highlight.response_id
        jump_off_native = 'NOT_ATTEMPTED; deterministic current-response tests only'
        settings_file_unchanged_pending_close_verification = $true
    }
    Write-Summary 'PASS' 'The prebuilt Settings Search flow, current-response highlight receipts, native draft-only edits, Cancel, and owned app lifecycle passed. Jump-off behavior was not claimed as native evidence; it remains covered by deterministic tests.'
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

function Invoke-Phase2FileSearchWorkflow($Contract) {
    $opened = Invoke-Phase2RootOpener $Contract
    $waitContext = Get-Phase2WaitContext
    $empty = Wait-JumpModeObservation {
        param($state)
        (Test-Phase2OwnerSnapshot $state $Contract $waitContext.owner_instance) -and
            $state.dialog_state.file_search_open -eq $true -and
            $state.dialog_state.file_search_query_char_count -eq 0 -and
            $state.dialog_state.file_search_status -ceq 'pending' -and
            $null -eq $state.dialog_state.file_search_search_id -and
            [long]$state.activation_count -eq [long]$waitContext.activation_count
    }.GetNewClosure() $CaptureTimeoutSeconds 'File Search opened with an empty query and no active search'

    $active = Start-Phase2Hints $Contract
    $queryTarget = Get-Phase2Target $active $Contract 'Search query' 'text_entry' 'form/query' -RequireHint
    $null = Save-Phase2OwnerCapture 16 'file_search_query_hint' $Contract {
        param($state)
        $state.dialog_state.file_search_open -eq $true -and
            $state.dialog_state.file_search_query_char_count -eq 0 -and
            $state.dialog_state.file_search_status -ceq 'pending' -and
            $null -eq $state.dialog_state.file_search_search_id -and
            @($state.navigation_targets | Where-Object {
                $_.eligible -and $_.label -ceq 'Search query' -and $_.kind -ceq 'text_entry' -and
                    (Test-Phase2SemanticIdentity $_ 'form/query')
            }).Count -eq 1
    }.GetNewClosure() 'active File Search query hint with no submitted search'
    Close-Phase2Hints $Contract | Out-Null
    $queryFocus = Complete-Phase2HintTarget $Contract 'Search query' 'text_entry' 'form/query'
    if ([string]$queryFocus.target.target_id -cne [string]$queryTarget.target_id -or
        [long]$queryFocus.observation.activation_count -ne [long]$waitContext.activation_count -or
        [string]$queryFocus.observation.query -cne [string]$waitContext.query) {
        throw 'File Search query focus did not preserve the exact target, ROOT query and action count'
    }
    $queryTargetId = [string]$queryFocus.target.target_id
    Send-NativeText 'phase2-file-search-safe-query' 'safe test'
    $typed = Wait-JumpModeObservation {
        param($state)
        (Test-Phase2OwnerSnapshot $state $Contract $waitContext.owner_instance) -and
            $state.dialog_state.file_search_open -eq $true -and
            $state.dialog_state.file_search_query_char_count -eq 9 -and
            $state.dialog_state.file_search_status -ceq 'pending' -and
            $null -eq $state.dialog_state.file_search_search_id -and
            [string]$state.focused_navigation_destination -ceq $queryTargetId -and
            [long]$state.activation_count -eq [long]$waitContext.activation_count -and
            [string]$state.query -ceq [string]$waitContext.query -and !$state.active_session
    }.GetNewClosure() $CaptureTimeoutSeconds 'File Search field accepted harmless native text without starting a search'

    $cancelFocus = Complete-Phase2HintTarget $Contract 'Cancel' 'button' 'form/actions/cancel'
    $cancelTargetId = [string]$cancelFocus.target.target_id
    Invoke-NativeKeyBatch 'phase2-file-search-idle-cancel-enter' ([uint16[]]@(0x0D)) ([string[]]@('Enter'))
    $idleCancel = Wait-JumpModeObservation {
        param($state)
        (Test-Phase2OwnerSnapshot $state $Contract $waitContext.owner_instance) -and
            !$state.active_session -and
            $state.dialog_state.file_search_open -eq $true -and
            $state.dialog_state.file_search_query_char_count -eq 9 -and
            $state.dialog_state.file_search_status -ceq 'pending' -and
            $null -eq $state.dialog_state.file_search_search_id -and
            [string]$state.focused_navigation_destination -ceq $cancelTargetId -and
            [long]$state.activation_count -eq [long]$waitContext.activation_count -and
            [string]$state.query -ceq [string]$waitContext.query
    }.GetNewClosure() $CaptureTimeoutSeconds 'idle File Search Cancel Enter left the dialog and search state unchanged'
    $capture = Save-Phase2OwnerCapture 17 'file_search_idle_cancel' $Contract {
        param($state)
        $state.dialog_state.file_search_open -eq $true -and
            $state.dialog_state.file_search_query_char_count -eq 9 -and
            $state.dialog_state.file_search_status -ceq 'pending' -and
            $null -eq $state.dialog_state.file_search_search_id -and
            [string]$state.focused_navigation_destination -ceq $cancelTargetId
    }.GetNewClosure() 'File Search idle Cancel left harmless query text unsubmitted' -Inactive

    Invoke-NativeKeyBatch 'phase2-file-search-escape-close' ([uint16[]]@(0x1B)) ([string[]]@('Escape'))
    $closed = Wait-JumpModeObservation {
        param($state)
        $staleFileSearchTargets = @($state.navigation_targets | Where-Object {
            $_.surface_identity.kind -ceq 'dialog' -and
                $_.surface_identity.dialog_kind -ceq 'file-search'
        })
        $state.root_focused -and $state.root_owner_allowed -and
            $state.navigation_owner.kind -ceq 'root' -and !$state.active_session -and
            $staleFileSearchTargets.Count -eq 0 -and
            [long]$state.activation_count -eq [long]$waitContext.activation_count -and
            [string]$state.query -ceq [string]$waitContext.query
    }.GetNewClosure() $CaptureTimeoutSeconds 'Escape closed idle File Search and returned navigation to ROOT without stale targets'
    Write-Event 'phase2_file_search_idle_cancel_closed_safely' @{
        opener_action = $opened.last_action.action_id
        owner_instance = $waitContext.owner_instance
        query_target_id = $queryTargetId
        query_char_count = $typed.dialog_state.file_search_query_char_count
        query_search_id = $typed.dialog_state.file_search_search_id
        idle_cancel_target_id = $cancelTargetId
        idle_cancel_status = $idleCancel.dialog_state.file_search_status
        activation_count = $closed.activation_count
        root_query = $closed.query
        stale_file_search_target_count = 0
        capture_indexes = @($phase2Captures | ForEach-Object { $_.capture_index })
        search_started = $false
        running_search_cancel_claimed = $false
        file_mutation = $false
    }
    Add-ReceiptBoundScreenshotAlias $capture 'file-search.png'
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
        'FileSearch' { Invoke-Phase2FileSearchWorkflow $contract }
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
            'SettingsSearch' { 'g3-m1b-prebuilt-artifact-validation' }
            'TargetRepair' { 'g3-m2b-prebuilt-artifact-validation' }
            'NoteReplace' { 'g3-m9b-prebuilt-artifact-validation' }
            default { 'm01-fixture-validation' }
        }
        status = $fixtureScenarioStatus
        expected_surface = if ($Scenario -in @('Phase1', 'Phase1Enabled', 'Phase1Disabled')) { 'typed isolated safe Phase 1 enabled/disabled fixture profiles' } elseif ($Scenario -eq 'Phase2') { "typed isolated Phase 2 $phase2SurfaceKey fixture and required inert stores" } elseif ($Scenario -eq 'SettingsSearch') { 'externally built, hash-bound isolated phase2-settings profile' } elseif ($Scenario -eq 'TargetRepair') { 'externally built, hash-bound isolated target-repair profile with inert saved action and folder records' } elseif ($Scenario -eq 'NoteReplace') { "externally built, hash-bound isolated $noteReplaceFixtureScenario profile with exact generated current-note sources" } else { 'typed isolated fixture profile with stable safe note data' }
        capture_target = $null
        capture_trigger = $null
        screenshot = $null
        pixel_width = $null
        pixel_height = $null
        reason = $fixtureScenarioReason
    }
    $runScenario = [ordered]@{
        scenario_id = if ($script:isFinalSmoke) { 'm15-final-smoke' } else { switch ($Scenario) {
            'QueryProbe' { 'm05-query-only' }
            { $_ -in @('Phase1', 'Phase1Enabled', 'Phase1Disabled') } {
                $suffix = if ($Scenario -eq 'Phase1Enabled' -and $Screen -ne 'All') { "-$($Screen.ToLowerInvariant())" } else { '' }
                "m08-$($Scenario.ToLowerInvariant())$suffix"
            }
            'Phase2' { "m13-phase2-$phase2SurfaceKey" }
            'SettingsSearch' { 'g3-m1b-settings-search' }
            'TargetRepair' { 'g3-m2b-target-repair' }
            'NoteReplace' { 'g3-m9b-note-replace' }
            default { 'm01-baseline-dashboard' }
        } }
        status = $scenarioStatus
        expected_surface = if ($script:isFinalSmoke) { 'M15 representative end-to-end native smoke: resized dashboard FillQuery and exact note 001 open; only catalog 05 and 09 are asserted' } else { switch ($Scenario) {
            'QueryProbe' { 'active Jump Mode query hint, query focus handoff, and unchanged action count' }
            { $_ -in @('Phase1', 'Phase1Enabled', 'Phase1Disabled') } { "native Phase 1 $Screen dashboard/grid filter, selection, cancellation, note owner, and disabled-gate behavior" }
            'Phase2' { "native production query opener, exact $phase2SurfaceKey owner, and safe observed input behavior" }
            'SettingsSearch' { 'exact settings:search route, current-response focus highlights, draft-only edits, and Cancel' }
            'TargetRepair' { 'exact saved custom-action and folder repair intents, owned native picker cancellation/selection, and explicit confirmation' }
            'NoteReplace' { 'current unsaved Markdown replacement, same-owner Undo/Redo, normal Save, source-only Reveal, and discard' }
            default { 'actual Multi Launcher ROOT viewport with isolated note dashboard' }
        } }
        capture_target = if ($Scenario -in @('TargetRepair', 'NoteReplace')) { $null } else { 'ROOT' }
        capture_trigger = if ($Scenario -eq 'TargetRepair') { 'same-pass current-response observations; no desktop capture' } elseif ($Scenario -eq 'NoteReplace') { 'payload-free current-pass response observations; no desktop capture' } elseif ($Scenario -in @('Phase1Enabled', 'Phase1Disabled', 'Phase2', 'SettingsSearch')) { 'f11' } elseif ($Scenario -eq 'Phase1') { 'child native F11 receipts' } else { 'startup' }
        screenshot = if ($screenshotPath) { $screenshotPath } else { $null }
        pixel_width = $screenshotWidth
        pixel_height = $screenshotHeight
        captures = if ($Scenario -in @('TargetRepair', 'NoteReplace')) { @() } elseif ($Scenario -in @('Phase2', 'SettingsSearch')) { @($phase2Captures.ToArray()) } else { @($phase1Captures.ToArray()) }
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
        milestone = $runMilestone
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
        'Phase2' { "phase2-$phase2SurfaceKey" }
        'SettingsSearch' { 'phase2-settings' }
        'TargetRepair' { 'target-repair' }
        'NoteReplace' { 'note-replace-enabled or note-replace-disabled' }
        default { 'baseline' }
    }
    if ($NativeOnly) {
        Write-Summary 'BLOCKED' "Runner is validating the supplied prebuilt $Scenario artifact manifest before any app launch."
    } else {
        Write-Summary 'BLOCKED' "Runner is preparing a fresh $Scenario fixture and validating its typed production semantics before launch."
    }
    Write-Event 'run_created' @{ run_id = $runId; profile = $profileRoot; interactive = $interactive; session_id = $sessionId; overall_timeout_seconds = $OverallTimeoutSeconds; screen_filter = $Screen }

    if ($Scenario -eq 'SettingsSearch' -and !$NativeOnly) {
        throw 'SettingsSearch requires -NativeOnly and a validated PrebuiltArtifactManifest; it never builds or generates fixtures.'
    }
    if ($Scenario -eq 'TargetRepair' -and !$NativeOnly) {
        throw 'TargetRepair requires -NativeOnly and a validated PrebuiltArtifactManifest; it never builds or generates fixtures.'
    }
    if ($Scenario -eq 'NoteReplace' -and !$NativeOnly) {
        throw 'NoteReplace requires -NativeOnly and a validated PrebuiltArtifactManifest; it never builds or generates fixtures.'
    }
    if ($NativeOnly -and ($Scenario -notin @('SettingsSearch', 'TargetRepair', 'NoteReplace') -or $FixturesOnly)) {
        throw 'NativeOnly is supported only for a SettingsSearch, TargetRepair, or NoteReplace app run, not fixture-only mode.'
    }

    if ($Scenario -eq 'Phase1' -and $FixturesOnly) {
        throw 'The Phase1 aggregate orchestrates native GUI child runs and cannot be fixture-only. Validate children separately with -Scenario Phase1Enabled -FixturesOnly and -Scenario Phase1Disabled -FixturesOnly.'
    }

    if ($NativeOnly) {
        Invoke-PrebuiltNativeScenario
    } elseif ($Scenario -eq 'Phase1') {
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
    if ($Scenario -eq 'Phase2') { $fixtureScenarioReason += " The typed Phase 2 $phase2SurfaceKey store profile and fixture-bound safety identity passed validation; Next/Cancel remain disabled and error notifications enabled." }
    if ($Scenario -eq 'Phase1Enabled') {
        $fixtureScenarioReason += ' The negative Jump Mode gate profile was rejected for the expected fixture invariant; Paste Sequence Next and Cancel remained disabled.'
    }
    if ($isFinalSmoke) { Test-Phase1FinalSmokeCaptureContract }
    $phase = 'fixture_checked'
    Update-Scenarios
    Write-Summary 'BLOCKED' 'Fixture validation passed typed production widget config and production note loader/search checks: 32 list results, 32 search results, and four underscore bucket tags. The production hotkey-route validator accepted the isolated chord. The profile has an explicit `notes` allowlist, empty custom actions, disabled screenshot side effects, and isolated notes/template paths.'

    if ($FixturesOnly) {
        if ($isFinalSmoke) { Initialize-NativeQueryProbe }
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
            request_id = if ($isFinalSmoke) { 'm15-final-smoke-dashboard-note-open' } else { switch ($Scenario) {
                'QueryProbe' { 'query-only-probe' }
                'Phase1Enabled' { "phase1-enabled-$($Screen.ToLowerInvariant())-catalog" }
                'Phase1Disabled' { 'phase1-disabled-regression' }
                'Phase2' { "phase2-$phase2SurfaceKey-native" }
                default { 'baseline-dashboard' }
            } }
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
                    surface = $phase2SurfaceKey
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
            milestone = $runMilestone
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
            fixture_profile = if ($Scenario -in @('SettingsSearch', 'TargetRepair')) { $profileRoot } else { 'profile' }
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
            if ($isFinalSmoke) { $appWindowMetadata = Resize-OwnedWindowForFinalSmoke }
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
    if ($prebuiltSettingsPath) {
        try {
            if (!(Test-Path -LiteralPath $prebuiltSettingsPath -PathType Leaf)) {
                throw 'the isolated settings.json disappeared during the Settings Search run'
            }
            $settingsHashAfterClose = (Get-FileHash -Algorithm SHA256 -LiteralPath $prebuiltSettingsPath).Hash.ToLowerInvariant()
            Write-Event 'settings_search_settings_file_after_owned_close' @{
                before_launch_sha256 = $prebuiltSettingsSha256
                before_first_input_sha256 = $script:prebuiltSettingsPreInputSha256
                unchanged_while_app_open_after_cancel = $script:settingsSearchNavigationHashVerified
                after_close_sha256 = $settingsHashAfterClose
                matches_before_launch = ($settingsHashAfterClose -ceq $prebuiltSettingsSha256)
                matches_before_first_input = if ($script:prebuiltSettingsPreInputSha256) { $settingsHashAfterClose -ceq $script:prebuiltSettingsPreInputSha256 } else { $null }
                post_close_difference_is_separate_from_live_navigation_check = $true
            }
        } catch {
            $runStatus = 'FAIL'
            if (!$failureText) { $failureText = $_.Exception.Message }
            $scenarioStatus = 'FAIL'
            $scenarioReason = $_.Exception.Message
            Update-Scenarios
            Write-Event 'settings_search_settings_file_changed' @{ message = $_.Exception.Message }
        }
    }
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
            milestone = $runMilestone
            status = $runStatus
            phase = $phase
            started_at_utc = $startedAt.ToString('O')
            finished_at_utc = $finishedAt.ToString('O')
            elapsed_ms = [int64]($finishedAt - $startedAt).TotalMilliseconds
            repository_root = $repoRoot
            source_head = if ($headSha) { $headSha } else { $null }
            source_branch = if ($sourceBranch) { $sourceBranch } else { $null }
            source_diff_sha256 = if ($sourceDiffSha256) { $sourceDiffSha256 } else { $null }
            interactive_session = $interactive
            session_id = $sessionId
            fixture_profile = if ($Scenario -in @('SettingsSearch', 'TargetRepair')) { $profileRoot } else { 'profile' }
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
            } elseif ($isFinalSmoke) {
                [ordered]@{ request = 'qa-framebuffer-request.json'; output_dir = 'capture'; source = 'two native ROOT/F11 receipts bound to their exact retained observations and resized owned client area'; count = $phase1Captures.Count; captures = @($phase1Captures.ToArray()); resized_window = $finalSmokeResizeMetadata }
            } elseif ($Scenario -eq 'Phase2') {
                [ordered]@{ request = 'qa-framebuffer-request.json'; output_dir = 'capture'; source = 'native ROOT/F11 receipts bound to captured retained observations'; surface = $Surface; count = $phase2Captures.Count; captures = @($phase2Captures.ToArray()) }
            } elseif ($Scenario -eq 'SettingsSearch') {
                [ordered]@{ request = 'qa-framebuffer-request.json'; output_dir = 'capture'; source = 'prebuilt artifact-bound native ROOT/F11 receipts with same-pass Settings Search observations'; surface = 'Settings'; count = $phase2Captures.Count; captures = @($phase2Captures.ToArray()); jump_off_native = 'NOT_ATTEMPTED' }
            } elseif ($Scenario -eq 'TargetRepair') {
                [ordered]@{ request = 'qa-framebuffer-request.json'; observation = 'jump-observation.json'; source = 'prebuilt artifact-bound current-pass ROOT repair Response observations; no desktop image capture'; count = 0; scenario = 'TargetRepair'; native_picker_observations = @('file-cancel', 'file-confirm', 'folder-confirm') }
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
            } elseif ($isFinalSmoke) {
                'The M15 representative smoke passed with exactly two receipt-bound ROOT captures: catalog 05 after the Dashboard FillQuery Enter action and catalog 09 after the exact note 001 open Enter action. This run does not assert the complete Phase 1 catalog. The owned window was resized without moving, changing z-order, or activating it; captured client dimensions matched the ROOT PNG scale, and process cleanup was verified.'
            } elseif ($Scenario -eq 'Phase1') {
                'Both isolated Phase 1 app runs passed: enabled catalog 01–14 and disabled-gate case 18. All fifteen screenshots are receipt-bound native ROOT/F11 captures, and each child process tree was verified gone.'
            } elseif ($Scenario -eq 'Phase2') {
                "The isolated $Surface production opener and safe native dialog workflow passed. All $($phase2Captures.Count) screenshots are bound to their matching ROOT/F11 captured observations, and owned-process cleanup was verified."
            } elseif ($Scenario -eq 'SettingsSearch') {
                'The exact settings:search route, same-pass real-control highlight receipts, draft-only slider edits, Cancel, unchanged isolated settings bytes, and owned-process cleanup passed. Jump-off native interaction was not attempted; deterministic real-response tests cover that branch.'
            } elseif ($Scenario -eq 'TargetRepair') {
                'The generated saved custom action and folder were repaired through exact current UI Responses and same-process owned native pickers. The file picker was cancelled once without changing the source, then explicitly confirmed; saved metadata, arguments, and alias remained exact, no target was launched, and owned-process cleanup was verified.'
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
            } elseif ($isFinalSmoke) {
                'The M15 representative smoke did not satisfy its two-capture Dashboard/note-open checks, owned-window resize/frame readiness, receipt/client-size binding, or cleanup assertion. See the event log, per-capture records, and failure detail.'
            } elseif ($Scenario -eq 'Phase1') {
                'One or both Phase 1 child scenarios did not satisfy all native input, retained capture, fixture identity, or owned cleanup assertions. See phase1-index.json and each child evidence directory.'
            } elseif ($Scenario -eq 'Phase2') {
                "The $Surface native workflow did not satisfy every production-action, dialog-state, receipt-binding, or owned-cleanup assertion. See the event log, per-capture records, and failure detail."
            } elseif ($Scenario -eq 'SettingsSearch') {
                'The validated prebuilt Settings Search workflow did not satisfy every exact route, current-response highlight, draft-only edit, Cancel, settings-file preservation, receipt, or owned-cleanup assertion. See the event log, per-capture records, and failure detail.'
            } elseif ($Scenario -eq 'TargetRepair') {
                'The validated prebuilt TargetRepair workflow did not satisfy every exact saved-record intent, owned native-picker identity, explicit confirmation, metadata preservation, or owned-cleanup assertion. See the event log, current QA observation, and failure detail.'
            } else {
                'Fixture validation and real ROOT capture completed, with owned-process cleanup recorded. Dashboard fixture content remains blocked until a readiness signal can establish that isolated note results are visible.'
            }
            Write-Summary $runStatus $blockedDetails
        }
        Write-Output "$runMilestone status: $runStatus"
        Write-Output "Evidence: $evidenceRoot"
        if ($runStatus -eq 'FAIL') { exit 1 }
    }
    if ($null -eq $previousTargetDirectory) { Remove-Item Env:CARGO_TARGET_DIR -ErrorAction SilentlyContinue } else { $env:CARGO_TARGET_DIR = $previousTargetDirectory }
}
