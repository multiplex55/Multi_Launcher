#requires -Version 7.0
[CmdletBinding()]
param(
    [switch]$FixturesOnly,
    [ValidateSet('Baseline', 'QueryProbe')][string]$Scenario = 'Baseline',
    [ValidateRange(5, 120)][int]$CaptureTimeoutSeconds = 45,
    [ValidateRange(1, 30)][int]$GracefulCloseSeconds = 6
)

$ErrorActionPreference = 'Stop'
$repoRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))
$cargoFeatures = 'dev-smoke-binaries,qa-framebuffer-capture'
$runStamp = [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssZ')
$runSuffix = [Guid]::NewGuid().ToString('N').Substring(0, 8)
$scenarioTag = if ($Scenario -eq 'QueryProbe') { 'm05_query_probe' } else { 'm01_baseline' }
$runId = "jump_${scenarioTag}_${runStamp}_$runSuffix"
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
$scenarioReason = if ($Scenario -eq 'QueryProbe') { 'Native Jump Mode query probe has not started.' } else { 'Baseline capture has not started.' }
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
}
'@
    Add-Type -TypeDefinition $nativeType -Language CSharp -ErrorAction Stop
}

function Read-JumpModeObservation {
    if (!(Test-Path -LiteralPath $jumpObservationPath -PathType Leaf)) { return $null }
    try {
        $observation = Get-Content -Raw -LiteralPath $jumpObservationPath | ConvertFrom-Json
    } catch {
        return $null
    }
    if ([int]$observation.process_pid -ne $processId) { throw 'Jump Mode observation PID does not match the retained app process' }
    if ([string]$observation.fixture_manifest_sha256 -ne $fixtureHash) { throw 'Jump Mode observation fixture hash does not match the verified isolated fixture' }
    return $observation
}

function Wait-JumpModeObservation([scriptblock]$Predicate, [int]$TimeoutSeconds, [string]$Description) {
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    while ([DateTime]::UtcNow -lt $deadline) {
        $process.Refresh()
        if ($process.HasExited) {
            $script:processExitCode = $process.ExitCode
            throw "isolated app exited while waiting for $Description (exit $script:processExitCode)"
        }
        $observation = Read-JumpModeObservation
        if ($null -ne $observation -and (& $Predicate $observation)) { return $observation }
        Start-Sleep -Milliseconds 100
    }
    throw "timed out after $TimeoutSeconds seconds waiting for $Description"
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
        $windowDeadline = [DateTime]::UtcNow.AddSeconds([Math]::Min($CaptureTimeoutSeconds, 15))
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
        $foregroundDeadline = [DateTime]::UtcNow.AddSeconds(3)
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
        scenario_id = 'm01-fixture-validation'
        status = $fixtureScenarioStatus
        expected_surface = 'typed isolated fixture profile with stable safe note data'
        capture_target = $null
        capture_trigger = $null
        screenshot = $null
        pixel_width = $null
        pixel_height = $null
        reason = $fixtureScenarioReason
    }
    $runScenario = [ordered]@{
        scenario_id = if ($Scenario -eq 'QueryProbe') { 'm05-query-only' } else { 'm01-baseline-dashboard' }
        status = $scenarioStatus
        expected_surface = if ($Scenario -eq 'QueryProbe') { 'active Jump Mode query hint, query focus handoff, and unchanged action count' } else { 'actual Multi Launcher ROOT viewport with isolated note dashboard' }
        capture_target = 'ROOT'
        capture_trigger = 'startup'
        screenshot = if ($screenshotPath) { $screenshotPath } else { $null }
        pixel_width = $screenshotWidth
        pixel_height = $screenshotHeight
        reason = $scenarioReason
    }
    Write-Json $scenariosPath @($fixtureScenario, $runScenario)
}

try {
    if (Test-Path -LiteralPath $evidenceRoot) { throw "refusing to reuse evidence path $evidenceRoot" }
    New-Item -ItemType Directory -Path $evidenceRoot | Out-Null
    New-Item -ItemType Directory -Path $screenshotsRoot | Out-Null
    [IO.File]::WriteAllText($eventsPath, '', [Text.UTF8Encoding]::new($false))
    [IO.File]::WriteAllText($stdoutPath, '', [Text.UTF8Encoding]::new($false))
    [IO.File]::WriteAllText($stderrPath, '', [Text.UTF8Encoding]::new($false))
    Write-Json $manifestPath ([ordered]@{
        schema_version = 1
        run_id = $runId
        milestone = if ($Scenario -eq 'QueryProbe') { 'M05' } else { 'M01' }
        status = $runStatus
        phase = $phase
        started_at_utc = $startedAt.ToString('O')
        repository_root = $repoRoot
        source_head = $null
        interactive_session = $interactive
        session_id = $sessionId
        fixture_profile = 'profile'
        fixture_manifest_sha256 = $null
        executable = $null
        build_receipt_identity = $null
        capture = $null
        cleanup = $null
    })
    Update-Scenarios
    $fixtureScenarioArgument = if ($Scenario -eq 'QueryProbe') { 'query-probe' } else { 'baseline' }
    Write-Summary 'BLOCKED' "Runner is preparing a fresh $Scenario fixture and validating its typed production semantics before launch."
    Write-Event 'run_created' @{ run_id = $runId; profile = $profileRoot; interactive = $interactive; session_id = $sessionId }

    $metadataOutput = & cargo metadata --no-deps --format-version 1 --manifest-path (Join-Path $repoRoot 'Cargo.toml') 2>> (Join-Path $evidenceRoot 'metadata.stderr.log')
    if ($LASTEXITCODE -ne 0) { throw 'cargo metadata failed; see metadata.stderr.log' }
    $metadata = ($metadataOutput -join [Environment]::NewLine) | ConvertFrom-Json
    $targetDirectory = [IO.Path]::GetFullPath([string]$metadata.target_directory)
    $env:CARGO_TARGET_DIR = $targetDirectory
    $headSha = (& git -C $repoRoot rev-parse HEAD).Trim()
    if ($LASTEXITCODE -ne 0) { throw 'git rev-parse HEAD failed' }

    $phase = 'fixture_validation'
    Write-Event 'fixture_tests_started' @{ manifest = (Join-Path $repoRoot 'Cargo.toml') }
    & cargo test --manifest-path (Join-Path $repoRoot 'Cargo.toml') --profile fast-dev --features $cargoFeatures --bin jump_mode_fixture 1> $testStdoutPath 2> $testStderrPath
    $fixtureTestsExitCode = $LASTEXITCODE
    Write-Event 'fixture_tests_completed' @{ exit_code = $fixtureTestsExitCode }
    if ($fixtureTestsExitCode -ne 0) { throw 'fixture generator tests failed; see fixture-tests logs' }

    Write-Event 'fixture_generation_started' @{ profile = $profileRoot }
    & cargo run --quiet --manifest-path (Join-Path $repoRoot 'Cargo.toml') --profile fast-dev --features $cargoFeatures --bin jump_mode_fixture -- generate $profileRoot $fixtureScenarioArgument 1> $fixtureStdoutPath 2> $fixtureStderrPath
    $fixtureGenerateExitCode = $LASTEXITCODE
    if ($fixtureGenerateExitCode -ne 0) { throw 'fixture generation failed; see fixture logs' }
    & cargo run --quiet --manifest-path (Join-Path $repoRoot 'Cargo.toml') --profile fast-dev --features $cargoFeatures --bin jump_mode_fixture -- verify $profileRoot $fixtureScenarioArgument 1>> $fixtureStdoutPath 2>> $fixtureStderrPath
    $fixtureVerifyExitCode = $LASTEXITCODE
    Write-Event 'fixture_generation_completed' @{ generate_exit_code = $fixtureGenerateExitCode; verify_exit_code = $fixtureVerifyExitCode }
    if ($fixtureVerifyExitCode -ne 0) { throw 'fixture verification failed; see fixture logs' }
    $fixtureManifestFile = Join-Path $profileRoot 'fixture_manifest.json'
    $fixtureHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $fixtureManifestFile).Hash.ToLowerInvariant()
    $fixtureScenarioStatus = 'PASS'
    $fixtureScenarioReason = 'Typed settings/dashboard and production pinned-query config passed; isolated production note loading/search returned all 32 stable note IDs and four underscore bucket tags; the production hotkey-route validator accepted the isolated fixture chord; side-effect safety checks passed.'
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
        & cargo build --manifest-path (Join-Path $repoRoot 'Cargo.toml') --profile fast-dev --features $cargoFeatures --bin multi_launcher 1> $buildStdoutPath 2> $buildStderrPath
        $buildExitCode = $LASTEXITCODE
        Write-Event 'app_build_completed' @{ exit_code = $buildExitCode }
        if ($buildExitCode -ne 0) { throw 'qa-framebuffer-capture app build failed; see build logs' }

        $executablePath = Join-Path $targetDirectory 'fast-dev\multi_launcher.exe'
        if (!(Test-Path -LiteralPath $executablePath -PathType Leaf)) { throw "built app executable is missing: $executablePath" }
        $executablePath = [IO.Path]::GetFullPath($executablePath)
        $executableHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $executablePath).Hash.ToLowerInvariant()
        $buildReceiptIdentity = "cargo-fast-dev;features=$cargoFeatures;head=$headSha;exe_sha256=$executableHash"

        if (Test-Path -LiteralPath $captureRoot) { throw 'QA framebuffer output leaf must be absent before app launch' }
        $qaRequestPath = Join-Path $evidenceRoot 'qa-framebuffer-request.json'
        $qaRequest = [ordered]@{
            run_id = $runId
            request_id = if ($Scenario -eq 'QueryProbe') { 'query-only-probe' } else { 'baseline-dashboard' }
            target = 'ROOT'
            trigger = 'startup'
            capture_count = 1
            diagnostics = $false
            output_dir = $captureRoot
            fixture_manifest_sha256 = $fixtureHash
            build_receipt_identity = $buildReceiptIdentity
        }
        if ($Scenario -eq 'QueryProbe') {
            $qaRequest.jump_mode_observation = [ordered]@{ wait_for_active_hints = $true }
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
            milestone = if ($Scenario -eq 'QueryProbe') { 'M05' } else { 'M01' }
            status = $runStatus
            phase = $phase
            started_at_utc = $startedAt.ToString('O')
            repository_root = $repoRoot
            source_head = $headSha
            interactive_session = $interactive
            session_id = $sessionId
            fixture_profile = 'profile'
            fixture_manifest_sha256 = $fixtureHash
            executable = [ordered]@{ path = $executablePath; sha256 = $executableHash; pid = $processId; started_at_utc = $processStartUtc.ToString('O') }
            build_receipt_identity = $buildReceiptIdentity
            capture = [ordered]@{ request = 'qa-framebuffer-request.json'; output_dir = 'capture'; source = 'qa-framebuffer-capture ROOT/Startup; CPU UI completion before GPU presentation'; scenario = $Scenario }
            cleanup = $null
        })
        if ($Scenario -eq 'QueryProbe') {
            Test-ObservedQueryTargetLookup
            Write-Event 'query_target_lookup_regression_passed' @{ target_id = 'fixture-query-target'; hint = 'AA' }
            Initialize-NativeQueryProbe
            Assert-OwnedForeground
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
            $deadline = [DateTime]::UtcNow.AddSeconds($CaptureTimeoutSeconds)
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
        } else {
            Write-Summary 'BLOCKED' 'App launched from the isolated profile. Waiting for a ROOT/Startup framebuffer receipt and a valid PNG.'

            $receiptPath = Join-Path $captureRoot 'receipt.json'
            $sourcePngPath = Join-Path $captureRoot 'framebuffer.png'
            $deadline = [DateTime]::UtcNow.AddSeconds($CaptureTimeoutSeconds)
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
            milestone = if ($Scenario -eq 'QueryProbe') { 'M05' } else { 'M01' }
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
            fixture_manifest_sha256 = $fixtureHash
            executable = if ($executablePath) { [ordered]@{ path = $executablePath; sha256 = $executableHash; pid = $processId; started_at_utc = if ($processStartUtc) { $processStartUtc.ToString('O') } else { $null } } } else { $null }
            build_receipt_identity = $buildReceiptIdentity
            capture = if ($captureReceipt) { [ordered]@{ request = 'qa-framebuffer-request.json'; output_dir = 'capture'; receipt = 'capture/receipt.json'; screenshot = $screenshotPath; width = $screenshotWidth; height = $screenshotHeight; semantics = $captureReceipt.capture_semantics } } else { $null }
            cleanup = $cleanup
            failure = $failureText
        }
        Write-Json $manifestPath $finalManifest
        if ($runStatus -eq 'PASS') {
            $passDetails = if ($FixturesOnly) {
                "Fixture validation passed. The $Scenario GUI scenario remains blocked because this invocation did not request an app launch."
            } elseif ($Scenario -eq 'QueryProbe') {
                'Fixture validation, receipt-bound active-query capture, native Shift+Space activation, map-derived query focus handoff, unchanged activation count, normal query text edit, and owned-process cleanup passed.'
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
            } else {
                'Fixture validation and real ROOT capture completed, with owned-process cleanup recorded. Dashboard fixture content remains blocked until a readiness signal can establish that isolated note results are visible.'
            }
            Write-Summary $runStatus $blockedDetails
        }
        $milestoneLabel = if ($Scenario -eq 'QueryProbe') { 'M05' } else { 'M01' }
        Write-Output "$milestoneLabel status: $runStatus"
        Write-Output "Evidence: $evidenceRoot"
        if ($runStatus -eq 'FAIL') { exit 1 }
    }
    if ($null -eq $previousTargetDirectory) { Remove-Item Env:CARGO_TARGET_DIR -ErrorAction SilentlyContinue } else { $env:CARGO_TARGET_DIR = $previousTargetDirectory }
}
