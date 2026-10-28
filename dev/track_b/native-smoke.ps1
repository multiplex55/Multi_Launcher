#requires -Version 7.0
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$RepositoryRoot,
    [Parameter(Mandatory = $true)][string]$ExecutablePath,
    [Parameter(Mandatory = $true)][ValidatePattern('^[0-9a-fA-F]{64}$')][string]$ExpectedExecutableSha256,
    [Parameter(Mandatory = $true)][string]$FixtureGeneratorPath,
    [Parameter(Mandatory = $true)][ValidatePattern('^[0-9a-fA-F]{64}$')][string]$ExpectedFixtureGeneratorSha256,
    [Parameter(Mandatory = $true)][string]$EvidenceDirectory,
    [ValidateRange(5, 90)][int]$StartupTimeoutSeconds = 45,
    [ValidateRange(1, 10)][int]$OutputDrainTimeoutSeconds = 5,
    [ValidateRange(1, 30)][int]$GracefulCloseSeconds = 12,
    [ValidateRange(1, 10)][int]$ForcedCleanupSeconds = 5
)

$ErrorActionPreference = 'Stop'
if (!$IsWindows) { throw 'Native app smoke requires Windows.' }
if (![Environment]::UserInteractive) { throw 'Native app smoke requires an interactive Windows session.' }

if ($null -eq ('TrackBNativeSmokeNative' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;
using System.Threading.Tasks;

[StructLayout(LayoutKind.Sequential)]
public struct TrackBNativeSmokeRect
{
    public int Left;
    public int Top;
    public int Right;
    public int Bottom;
}

public sealed class TrackBNativeSmokeWindowInfo
{
    public IntPtr Handle;
    public uint ProcessId;
    public bool Visible;
    public string Title;
    public string ClassName;
    public int ClientWidth;
    public int ClientHeight;
}

[UnmanagedFunctionPointer(CallingConvention.Winapi)]
public delegate bool TrackBNativeSmokeEnumWindowsCallback(IntPtr window, IntPtr lParam);

public static class TrackBNativeSmokeNative
{
    [DllImport("user32.dll", EntryPoint = "EnumWindows", ExactSpelling = true, SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool EnumWindows(TrackBNativeSmokeEnumWindowsCallback callback, IntPtr lParam);

    [DllImport("user32.dll", EntryPoint = "GetForegroundWindow", ExactSpelling = true)]
    public static extern IntPtr GetForegroundWindow();

    [DllImport("user32.dll", EntryPoint = "GetWindowRect", ExactSpelling = true, SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    public static extern bool GetWindowRect(IntPtr window, out TrackBNativeSmokeRect rect);

    [DllImport("user32.dll", EntryPoint = "IsWindowVisible", ExactSpelling = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    public static extern bool IsWindowVisible(IntPtr window);

    [DllImport("user32.dll", EntryPoint = "GetWindowThreadProcessId", ExactSpelling = true, SetLastError = true)]
    public static extern uint GetWindowThreadProcessId(IntPtr window, out uint processId);

    [DllImport("user32.dll", EntryPoint = "GetWindowTextW", ExactSpelling = true, CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern int GetWindowTextW(IntPtr window, StringBuilder text, int maximumCount);

    [DllImport("user32.dll", EntryPoint = "GetClassNameW", ExactSpelling = true, CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern int GetClassNameW(IntPtr window, StringBuilder className, int maximumCount);

    [DllImport("user32.dll", EntryPoint = "GetClientRect", ExactSpelling = true, SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool GetClientRect(IntPtr window, out TrackBNativeSmokeRect rect);

    [DllImport("user32.dll", EntryPoint = "PostMessageW", ExactSpelling = true, SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    public static extern bool PostMessageW(IntPtr window, uint message, UIntPtr wParam, IntPtr lParam);

    [DllImport("user32.dll", EntryPoint = "SendMessageTimeoutW", ExactSpelling = true, SetLastError = true)]
    public static extern IntPtr SendMessageTimeoutW(
        IntPtr window, uint message, UIntPtr wParam, IntPtr lParam,
        uint flags, uint timeoutMilliseconds, out UIntPtr result);

    public static TrackBNativeSmokeWindowInfo[] EnumerateTopLevelWindows(uint expectedProcessId)
    {
        var windows = new List<TrackBNativeSmokeWindowInfo>();
        TrackBNativeSmokeEnumWindowsCallback callback = delegate(IntPtr window, IntPtr lParam)
        {
            uint processId;
            GetWindowThreadProcessId(window, out processId);
            if (processId != expectedProcessId)
                return true;

            var title = new StringBuilder(512);
            GetWindowTextW(window, title, title.Capacity);
            var className = new StringBuilder(256);
            GetClassNameW(window, className, className.Capacity);
            TrackBNativeSmokeRect clientRect;
            int clientWidth = 0;
            int clientHeight = 0;
            if (GetClientRect(window, out clientRect))
            {
                clientWidth = clientRect.Right - clientRect.Left;
                clientHeight = clientRect.Bottom - clientRect.Top;
            }

            windows.Add(new TrackBNativeSmokeWindowInfo
            {
                Handle = window,
                ProcessId = processId,
                Visible = IsWindowVisible(window),
                Title = title.ToString(),
                ClassName = className.ToString(),
                ClientWidth = clientWidth,
                ClientHeight = clientHeight
            });
            return true;
        };

        if (!EnumWindows(callback, IntPtr.Zero))
            throw new Win32Exception(Marshal.GetLastWin32Error(), "Could not enumerate top-level windows.");
        GC.KeepAlive(callback);
        return windows.ToArray();
    }
}

public sealed class TrackBNativeSmokeOutputCapture
{
    private long _bytesRead;
    private long _bytesRecorded;

    public string Path { get; private set; }
    public Task Completion { get; private set; }
    public long BytesRead { get { return Interlocked.Read(ref _bytesRead); } }
    public long BytesRecorded { get { return Interlocked.Read(ref _bytesRecorded); } }

    private TrackBNativeSmokeOutputCapture(string path)
    {
        Path = path;
    }

    public static TrackBNativeSmokeOutputCapture Start(Stream source, string path)
    {
        var capture = new TrackBNativeSmokeOutputCapture(path);
        capture.Completion = capture.CopyAsync(source);
        return capture;
    }

    private async Task CopyAsync(Stream source)
    {
        byte[] buffer = new byte[65536];
        using (var destination = new FileStream(Path, FileMode.CreateNew, FileAccess.Write, FileShare.Read, buffer.Length, FileOptions.Asynchronous | FileOptions.SequentialScan))
        {
            int count;
            while ((count = await source.ReadAsync(buffer, 0, buffer.Length).ConfigureAwait(false)) != 0)
            {
                Interlocked.Add(ref _bytesRead, count);
                await destination.WriteAsync(buffer, 0, count).ConfigureAwait(false);
                Interlocked.Add(ref _bytesRecorded, count);
            }
            await destination.FlushAsync().ConfigureAwait(false);
        }
    }
}
'@ | Out-Null
}

function Get-FileBinding {
    param([Parameter(Mandatory = $true)][string]$Path)
    if (!(Test-Path -LiteralPath $Path -PathType Leaf)) { throw "Required file is missing: $Path" }
    $file = Get-Item -LiteralPath $Path -ErrorAction Stop
    [pscustomobject][ordered]@{
        Path = [IO.Path]::GetFullPath($Path)
        LengthBytes = [Int64]$file.Length
        Sha256 = (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
    }
}

function Get-ExactIdentityState {
    param([Parameter(Mandatory = $true)][int]$ProcessId, [Parameter(Mandatory = $true)][DateTime]$CreationUtc)
    try {
        $candidate = [Diagnostics.Process]::GetProcessById($ProcessId)
        try {
            $candidateStart = $candidate.StartTime.ToUniversalTime()
            if ([Math]::Abs(($candidateStart - $CreationUtc.ToUniversalTime()).TotalMilliseconds) -le 5) { return 'Alive' }
            return 'Reused'
        }
        finally { $candidate.Dispose() }
    }
    catch [ArgumentException] { return 'Absent' }
    catch { return 'Unknown' }
}

function Get-ValidatedRootWindowSet {
    param(
        [Parameter(Mandatory = $true)][int]$ProcessId,
        [Parameter(Mandatory = $true)][int]$ExpectedClientWidth,
        [Parameter(Mandatory = $true)][int]$ExpectedClientHeight
    )
    $windows = @([TrackBNativeSmokeNative]::EnumerateTopLevelWindows([uint32]$ProcessId))
    $roots = @($windows | Where-Object {
        $_.Visible -and
        [string]::Equals($_.Title, 'Multi Lnchr', [StringComparison]::Ordinal) -and
        $_.ClientWidth -ge $ExpectedClientWidth -and
        $_.ClientHeight -ge $ExpectedClientHeight
    })
    $details = @($windows | ForEach-Object {
        [pscustomobject][ordered]@{
            Handle = ('0x{0:x}' -f $_.Handle.ToInt64())
            ProcessId = [int]$_.ProcessId
            Visible = [bool]$_.Visible
            Title = $_.Title
            ClassName = $_.ClassName
            ClientWidth = [int]$_.ClientWidth
            ClientHeight = [int]$_.ClientHeight
            IsLauncherRoot = ($roots.Count -eq 1 -and $_.Handle -eq $roots[0].Handle)
        }
    })
    [pscustomobject][ordered]@{ Roots = $roots; TopLevelWindows = $details }
}

function Start-RedirectedProcess {
    param(
        [Parameter(Mandatory = $true)][Diagnostics.ProcessStartInfo]$StartInfo,
        [Parameter(Mandatory = $true)][string]$StdoutPath,
        [Parameter(Mandatory = $true)][string]$StderrPath
    )
    $child = [Diagnostics.Process]::new()
    $child.StartInfo = $StartInfo
    if (!$child.Start()) { $child.Dispose(); throw "Could not start executable: $($StartInfo.FileName)" }
    $startedUtc = $child.StartTime.ToUniversalTime()
    $stdoutCapture = [TrackBNativeSmokeOutputCapture]::Start($child.StandardOutput.BaseStream, $StdoutPath)
    $stderrCapture = [TrackBNativeSmokeOutputCapture]::Start($child.StandardError.BaseStream, $StderrPath)
    return [pscustomobject]@{
        Process = $child
        StartedUtc = $startedUtc
        StdoutCapture = $stdoutCapture
        StderrCapture = $stderrCapture
        OutputDrain = $null
    }
}

function Save-ProcessOutput {
    param(
        [Parameter(Mandatory = $true)]$Run,
        [Parameter(Mandatory = $true)][ValidateRange(1, 10)][int]$DrainTimeoutSeconds
    )
    if (!$Run.Process.HasExited) { throw 'Cannot drain a still-running child process.' }
    if ($Run.OutputDrain) { return $Run.OutputDrain }
    $allTasks = [Threading.Tasks.Task]::WhenAll([Threading.Tasks.Task[]]@(
        $Run.StdoutCapture.Completion,
        $Run.StderrCapture.Completion
    ))
    $wait = [Diagnostics.Stopwatch]::StartNew()
    $waitCompleted = $false
    $drainError = $null
    try { $waitCompleted = $allTasks.Wait($DrainTimeoutSeconds * 1000) }
    catch { $waitCompleted = $allTasks.IsCompleted; $drainError = $_.Exception.ToString() }
    if ($waitCompleted -and !$drainError) {
        try { $allTasks.GetAwaiter().GetResult() }
        catch { $drainError = $_.Exception.ToString() }
    }
    $wait.Stop()

    function Get-CaptureReceipt {
        param([Parameter(Mandatory = $true)]$Capture)
        $task = $Capture.Completion
        $completed = $task.IsCompleted
        $fileBytes = $null
        $hash = $null
        if (Test-Path -LiteralPath $Capture.Path -PathType Leaf) {
            $fileBytes = [Int64](Get-Item -LiteralPath $Capture.Path -ErrorAction Stop).Length
            if ($completed) { $hash = (Get-FileHash -LiteralPath $Capture.Path -Algorithm SHA256).Hash.ToLowerInvariant() }
        }
        $errorText = if ($task.IsFaulted) { $task.Exception.ToString() } elseif ($task.IsCanceled) { 'Raw output capture was canceled.' } else { $null }
        return [pscustomobject][ordered]@{
            Path = $Capture.Path
            TaskState = $task.Status.ToString()
            Completed = $completed
            BytesRead = [Int64]$Capture.BytesRead
            BytesRecorded = [Int64]$Capture.BytesRecorded
            FileBytesAtReceipt = $fileBytes
            ReadMatchesRecorded = [bool]($Capture.BytesRead -eq $Capture.BytesRecorded)
            RecordedMatchesFile = [bool]($null -ne $fileBytes -and $Capture.BytesRecorded -eq $fileBytes)
            Sha256 = $hash
            Error = $errorText
        }
    }

    $stdout = Get-CaptureReceipt $Run.StdoutCapture
    $stderr = Get-CaptureReceipt $Run.StderrCapture
    $verified = $waitCompleted -and !$drainError -and
        $stdout.TaskState -eq 'RanToCompletion' -and $stderr.TaskState -eq 'RanToCompletion' -and
        $stdout.ReadMatchesRecorded -and $stdout.RecordedMatchesFile -and
        $stderr.ReadMatchesRecorded -and $stderr.RecordedMatchesFile
    $Run.OutputDrain = [pscustomobject][ordered]@{
        DrainVerified = [bool]$verified
        DrainTimeoutSeconds = $DrainTimeoutSeconds
        WaitMilliseconds = [Math]::Round($wait.Elapsed.TotalMilliseconds, 3)
        CombinedTaskState = $allTasks.Status.ToString()
        Error = $drainError
        Stdout = $stdout
        Stderr = $stderr
    }
    return $Run.OutputDrain
}

function Save-OwnedWindowCapture {
    param(
        [Parameter(Mandatory = $true)][IntPtr]$Window,
        [Parameter(Mandatory = $true)][int]$ProcessId,
        [Parameter(Mandatory = $true)][string]$Path
    )
    $rect = [TrackBNativeSmokeRect]::new()
    if (![TrackBNativeSmokeNative]::GetWindowRect($Window, [ref]$rect)) {
        throw 'Could not read the owned main-window bounds for capture.'
    }
    $width = $rect.Right - $rect.Left
    $height = $rect.Bottom - $rect.Top
    if ($width -le 0 -or $height -le 0 -or $width -gt 8192 -or $height -gt 8192) {
        throw "Owned main-window bounds are invalid for capture: ${width}x${height}."
    }
    $foreground = [TrackBNativeSmokeNative]::GetForegroundWindow()
    [uint32]$foregroundProcessId = 0
    $null = [TrackBNativeSmokeNative]::GetWindowThreadProcessId($foreground, [ref]$foregroundProcessId)
    if ($foreground -ne $Window -or $foregroundProcessId -ne [uint32]$ProcessId) {
        return [pscustomobject][ordered]@{
            Status = 'Unavailable'
            Path = $Path
            Reason = 'Candidate was not the foreground window; desktop pixels were not collected.'
            Hwnd = ('0x{0:x}' -f $Window.ToInt64())
            ForegroundHwnd = ('0x{0:x}' -f $foreground.ToInt64())
            ForegroundProcessId = $foregroundProcessId
            Width = $width
            Height = $height
            Bytes = $null
            Sha256 = $null
        }
    }

    Add-Type -AssemblyName System.Drawing.Common
    $bitmap = [Drawing.Bitmap]::new($width, $height)
    $graphics = $null
    try {
        $graphics = [Drawing.Graphics]::FromImage($bitmap)
        $graphics.CopyFromScreen(
            $rect.Left, $rect.Top, 0, 0,
            [Drawing.Size]::new($width, $height),
            [Drawing.CopyPixelOperation]::SourceCopy
        )
        $bitmap.Save($Path, [Drawing.Imaging.ImageFormat]::Png)
    }
    finally {
        if ($graphics) { $graphics.Dispose() }
        $bitmap.Dispose()
    }
    if (!(Test-Path -LiteralPath $Path -PathType Leaf)) { throw 'Owned-window capture did not create its PNG.' }
    $bytes = [IO.File]::ReadAllBytes($Path)
    $signature = [byte[]](137, 80, 78, 71, 13, 10, 26, 10)
    if ($bytes.Length -lt 24 -or [Text.Encoding]::ASCII.GetString($bytes, 12, 4) -cne 'IHDR') {
        throw 'Owned-window capture is not a valid PNG.'
    }
    for ($index = 0; $index -lt $signature.Length; $index++) {
        if ($bytes[$index] -ne $signature[$index]) { throw 'Owned-window capture has an invalid PNG signature.' }
    }
    $foregroundAfter = [TrackBNativeSmokeNative]::GetForegroundWindow()
    [uint32]$foregroundProcessIdAfter = 0
    $null = [TrackBNativeSmokeNative]::GetWindowThreadProcessId($foregroundAfter, [ref]$foregroundProcessIdAfter)
    if ($foregroundAfter -ne $Window -or $foregroundProcessIdAfter -ne [uint32]$ProcessId) {
        Remove-Item -LiteralPath $Path -Force -ErrorAction SilentlyContinue
        throw 'Candidate lost foreground ownership during the window capture; screenshot was discarded.'
    }
    $pngWidth = ([uint32]$bytes[16] -shl 24) -bor ([uint32]$bytes[17] -shl 16) -bor ([uint32]$bytes[18] -shl 8) -bor [uint32]$bytes[19]
    $pngHeight = ([uint32]$bytes[20] -shl 24) -bor ([uint32]$bytes[21] -shl 16) -bor ([uint32]$bytes[22] -shl 8) -bor [uint32]$bytes[23]
    if ($pngWidth -eq 0 -or $pngHeight -eq 0) { throw 'Owned-window PNG has empty dimensions.' }
    return [pscustomobject][ordered]@{
        Status = 'Captured'
        Path = $Path
        Method = 'System.Drawing.Graphics.CopyFromScreen on the exact visible foreground-owned window bounds.'
        Hwnd = ('0x{0:x}' -f $Window.ToInt64())
        ProcessId = $ProcessId
        ForegroundProcessId = $foregroundProcessId
        Bounds = [pscustomobject]@{ Left = $rect.Left; Top = $rect.Top; Right = $rect.Right; Bottom = $rect.Bottom }
        Width = [int]$pngWidth
        Height = [int]$pngHeight
        Bytes = $bytes.Length
        Sha256 = (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
    }
}

$repoRoot = [IO.Path]::GetFullPath((Resolve-Path -LiteralPath $RepositoryRoot -ErrorAction Stop).ProviderPath)
$exePath = [IO.Path]::GetFullPath((Resolve-Path -LiteralPath $ExecutablePath -ErrorAction Stop).ProviderPath)
$generatorPath = [IO.Path]::GetFullPath((Resolve-Path -LiteralPath $FixtureGeneratorPath -ErrorAction Stop).ProviderPath)
$evidenceRoot = [IO.Path]::GetFullPath($EvidenceDirectory)
if ([string]::IsNullOrWhiteSpace([IO.Path]::GetPathRoot($evidenceRoot))) { throw 'Evidence directory must be an absolute filesystem path.' }
$repoPrefix = $repoRoot.TrimEnd('\') + '\'
if ($evidenceRoot.Equals($repoRoot, [StringComparison]::OrdinalIgnoreCase) -or
    $evidenceRoot.StartsWith($repoPrefix, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Native smoke evidence must remain outside the repository.'
}
if (Test-Path -LiteralPath $evidenceRoot) { throw "Evidence directory already exists; refusing to overwrite it: $evidenceRoot" }
[IO.Directory]::CreateDirectory($evidenceRoot) | Out-Null

$profileRoot = Join-Path $evidenceRoot 'isolated-profile'
$settingsPath = Join-Path $profileRoot 'settings.json'
$resultPath = Join-Path $evidenceRoot 'native-smoke-result.json'
$generatorStdoutPath = Join-Path $evidenceRoot 'fixture-generator.stdout.raw'
$generatorStderrPath = Join-Path $evidenceRoot 'fixture-generator.stderr.raw'
$appStdoutPath = Join-Path $evidenceRoot 'app.stdout.raw'
$appStderrPath = Join-Path $evidenceRoot 'app.stderr.raw'
$windowCapturePath = Join-Path $evidenceRoot 'owned-window.png'

$result = [ordered]@{
    SchemaVersion = 1
    Status = 'FAIL'
    StartedUtc = [DateTimeOffset]::UtcNow.ToString('o')
    EndedUtc = $null
    Error = $null
    RepositoryRoot = $repoRoot
    Executable = $null
    FixtureGenerator = $null
    IsolatedProfile = $profileRoot
    SettingsPath = $settingsPath
    SettingsSha256Before = $null
    SettingsSha256After = $null
    ExpectedWindowSize = $null
    PasteSequenceNextHotkey = $null
    PasteSequenceCancelHotkey = $null
    FixtureGeneration = $null
    AppProcess = $null
    AppWindow = $null
    OwnedTopLevelWindowsAtSelection = @()
    WindowCapture = $null
    GracefulClose = $null
    CleanupCloseAttempt = $null
    FinalIdentityState = 'Unknown'
    AppOutputDrainVerified = $false
    AppStdout = $null
    AppStderr = $null
}
$generatorRun = $null
$appRun = $null
$forcedCleanup = $false
$windowHandle = [IntPtr]::Zero
$expectedClientWidth = 900
$expectedClientHeight = 560

try {
    $result.Executable = Get-FileBinding $exePath
    $result.FixtureGenerator = Get-FileBinding $generatorPath
    if ($result.Executable.Sha256 -cne $ExpectedExecutableSha256.ToLowerInvariant()) {
        throw 'Candidate app SHA-256 differs from its accepted artifact binding.'
    }
    if ($result.FixtureGenerator.Sha256 -cne $ExpectedFixtureGeneratorSha256.ToLowerInvariant()) {
        throw 'Fixture generator SHA-256 differs from its accepted artifact binding.'
    }

    if (Test-Path -LiteralPath $profileRoot) { throw "Isolated profile path already exists: $profileRoot" }
    $generatorInfo = [Diagnostics.ProcessStartInfo]::new()
    $generatorInfo.FileName = $generatorPath
    $generatorInfo.WorkingDirectory = $repoRoot
    $generatorInfo.UseShellExecute = $false
    $generatorInfo.CreateNoWindow = $true
    $generatorInfo.RedirectStandardOutput = $true
    $generatorInfo.RedirectStandardError = $true
    foreach ($argument in @('generate', $profileRoot, 'baseline')) { $generatorInfo.ArgumentList.Add($argument) }
    $generatorRun = Start-RedirectedProcess $generatorInfo $generatorStdoutPath $generatorStderrPath
    $generatorIdentity = [pscustomobject][ordered]@{
        ProcessId = $generatorRun.Process.Id
        CreationTimeUtc = $generatorRun.StartedUtc.ToString('o')
        Executable = $generatorPath
    }
    $result.FixtureGeneration = [pscustomobject][ordered]@{
        Process = $generatorIdentity
        ExitCode = $null
        CleanupIdentityState = 'Running'
        OutputDrainVerified = $false
        Output = $null
    }
    if (!$generatorRun.Process.WaitForExit(30000)) {
        $generatorRun.Process.Kill($true)
        $null = $generatorRun.Process.WaitForExit(5000)
        $generatorIdentity | Add-Member NoteProperty Timeout true
        throw 'Fixture generator exceeded its 30-second bound.'
    }
    $generatorRun.Process.Refresh()
    $generatorDrain = Save-ProcessOutput $generatorRun $OutputDrainTimeoutSeconds
    $generatorCleanupState = Get-ExactIdentityState $generatorRun.Process.Id $generatorRun.StartedUtc
    $result.FixtureGeneration.ExitCode = $generatorRun.Process.ExitCode
    $result.FixtureGeneration.CleanupIdentityState = $generatorCleanupState
    $result.FixtureGeneration.OutputDrainVerified = $generatorDrain.DrainVerified
    $result.FixtureGeneration.Output = $generatorDrain
    if ($generatorRun.Process.ExitCode -ne 0 -or $generatorCleanupState -notin @('Absent', 'Reused') -or !$generatorDrain.DrainVerified) {
        throw 'Fixture generator did not exit successfully with its exact process identity retired.'
    }
    $generatorRun.Process.Dispose()
    $generatorRun = $null

    if (!(Test-Path -LiteralPath $settingsPath -PathType Leaf)) { throw 'Fixture generator did not create isolated settings.json.' }
    $settings = Get-Content -Raw -LiteralPath $settingsPath -Encoding utf8 | ConvertFrom-Json -Depth 40
    if ($null -eq $settings.paste_sequence) {
        throw 'Fixture settings are missing Paste Sequence preferences before app construction.'
    }
    $nextHotkeyProperty = $settings.paste_sequence.PSObject.Properties['next_hotkey']
    $cancelHotkeyProperty = $settings.paste_sequence.PSObject.Properties['cancel_hotkey']
    if ($null -eq $nextHotkeyProperty -or $null -eq $cancelHotkeyProperty) {
        throw 'Fixture settings must explicitly contain Paste Sequence Next and Cancel shortcut fields before app construction.'
    }
    if ($null -ne $nextHotkeyProperty.Value -or $null -ne $cancelHotkeyProperty.Value) {
        throw 'Paste Sequence Next and Cancel shortcuts must be null in settings before app construction.'
    }
    $fixtureWindowSize = @($settings.window_size)
    if ($fixtureWindowSize.Count -ne 2 -or
        [int]$fixtureWindowSize[0] -ne $expectedClientWidth -or
        [int]$fixtureWindowSize[1] -ne $expectedClientHeight) {
        throw 'Baseline fixture must request its expected 900x560 launcher client window before app construction.'
    }
    $result.ExpectedWindowSize = [pscustomobject][ordered]@{
        Width = $expectedClientWidth
        Height = $expectedClientHeight
    }
    $result.PasteSequenceNextHotkey = $nextHotkeyProperty.Value
    $result.PasteSequenceCancelHotkey = $cancelHotkeyProperty.Value
    $result.SettingsSha256Before = (Get-FileHash -LiteralPath $settingsPath -Algorithm SHA256).Hash.ToLowerInvariant()

    $appInfo = [Diagnostics.ProcessStartInfo]::new()
    $appInfo.FileName = $exePath
    $appInfo.WorkingDirectory = $profileRoot
    $appInfo.UseShellExecute = $false
    $appInfo.CreateNoWindow = $false
    $appInfo.WindowStyle = [Diagnostics.ProcessWindowStyle]::Normal
    $appInfo.RedirectStandardOutput = $true
    $appInfo.RedirectStandardError = $true
    $appInfo.Environment['ML_NOTES_DIR'] = Join-Path $profileRoot 'notes'
    $appInfo.Environment['ML_NOTE_TEMPLATES_DIR'] = Join-Path $profileRoot 'templates'
    $appInfo.Environment.Remove('ML_TRACK_C_BENCH_MODE') | Out-Null
    $appInfo.Environment.Remove('ML_QA_FRAMEBUFFER_REQUEST_MANIFEST') | Out-Null
    $appRun = Start-RedirectedProcess $appInfo $appStdoutPath $appStderrPath
    $process = $appRun.Process
    $processId = $process.Id
    $processCreationUtc = $appRun.StartedUtc
    $result.AppProcess = [pscustomobject][ordered]@{
        ProcessId = $processId
        CreationTimeUtc = $processCreationUtc.ToString('o')
        SessionId = $process.SessionId
        Executable = $exePath
        WorkingDirectory = $profileRoot
    }

    $windowDeadline = [DateTimeOffset]::UtcNow.AddSeconds($StartupTimeoutSeconds)
    $windowReady = $false
    $responsivenessResult = [UIntPtr]::Zero
    while ([DateTimeOffset]::UtcNow -lt $windowDeadline) {
        $process.Refresh()
        if ($process.HasExited) { throw "Candidate app exited before publishing a responsive main window (exit $($process.ExitCode))." }
        $windowSet = Get-ValidatedRootWindowSet $processId $expectedClientWidth $expectedClientHeight
        $result.OwnedTopLevelWindowsAtSelection = $windowSet.TopLevelWindows
        if ($windowSet.Roots.Count -gt 1) {
            throw 'Candidate published multiple visible launcher root windows matching the exact PID, title, and fixture client bounds.'
        }
        if ($windowSet.Roots.Count -eq 1) {
            $candidateWindow = $windowSet.Roots[0].Handle
            $probe = [TrackBNativeSmokeNative]::SendMessageTimeoutW(
                $candidateWindow, [uint32]0, [UIntPtr]::Zero, [IntPtr]::Zero,
                [uint32]0x00000003, [uint32]1000, [ref]$responsivenessResult)
            if ($probe -ne [IntPtr]::Zero) {
                $windowHandle = $candidateWindow
                $windowReady = $true
                break
            }
        }
        Start-Sleep -Milliseconds 200
    }
    if (!$windowReady) { throw "Candidate app did not publish a responsive visible main window within $StartupTimeoutSeconds seconds." }
    $result.AppWindow = [pscustomobject][ordered]@{
        Handle = ('0x{0:x}' -f $windowHandle.ToInt64())
        ProcessId = $processId
        Visible = [TrackBNativeSmokeNative]::IsWindowVisible($windowHandle)
        Title = $windowSet.Roots[0].Title
        ClassName = $windowSet.Roots[0].ClassName
        ClientWidth = [int]$windowSet.Roots[0].ClientWidth
        ClientHeight = [int]$windowSet.Roots[0].ClientHeight
        ExpectedClientWidth = $expectedClientWidth
        ExpectedClientHeight = $expectedClientHeight
        WmNullResponded = $true
        Probe = 'SendMessageTimeoutW(WM_NULL, SMTO_BLOCK|SMTO_ABORTIFHUNG, 1000ms)'
        ReadyAtUtc = [DateTimeOffset]::UtcNow.ToString('o')
    }
    try {
        $result.WindowCapture = Save-OwnedWindowCapture $windowHandle $processId $windowCapturePath
    }
    catch {
        $result.WindowCapture = [pscustomobject][ordered]@{
            Status = 'Unavailable'
            Path = $windowCapturePath
            Reason = '{0}: {1}' -f $_.Exception.GetType().FullName, $_.Exception.Message
        }
    }

    $process.Refresh()
    if ($process.HasExited -or $process.Id -ne $processId -or
        [Math]::Abs(($process.StartTime.ToUniversalTime() - $processCreationUtc).TotalMilliseconds) -gt 5) {
        throw 'Retained app Process object no longer matches its initial PID/creation identity.'
    }
    $windowSet = Get-ValidatedRootWindowSet $processId $expectedClientWidth $expectedClientHeight
    $retainedRoot = @($windowSet.Roots | Where-Object { $_.Handle -eq $windowHandle })
    if ($retainedRoot.Count -ne 1) {
        throw 'Selected launcher root window no longer matches the exact PID, title, and fixture client bounds before normal close.'
    }
    $closeRequestedUtc = [DateTimeOffset]::UtcNow
    $closeRequested = [TrackBNativeSmokeNative]::PostMessageW(
        $windowHandle, [uint32]0x0010, [UIntPtr]::Zero, [IntPtr]::Zero)
    $gracefulExited = $closeRequested -and $process.WaitForExit($GracefulCloseSeconds * 1000)
    if (!$gracefulExited) {
        $process.Refresh()
        if (!$process.HasExited -and [Math]::Abs(($process.StartTime.ToUniversalTime() - $processCreationUtc).TotalMilliseconds) -le 5) {
            $forcedCleanup = $true
            $process.Kill($true)
            $null = $process.WaitForExit($ForcedCleanupSeconds * 1000)
        }
    }
    $process.Refresh()
    $processExited = $process.HasExited
    $exitCode = if ($processExited) { $process.ExitCode } else { $null }
    $identityState = Get-ExactIdentityState $processId $processCreationUtc
    $result.FinalIdentityState = $identityState
    $result.GracefulClose = [pscustomobject][ordered]@{
        RequestedAtUtc = $closeRequestedUtc.ToString('o')
        Method = 'PostMessageW(WM_CLOSE) to revalidated launcher root HWND'
        TargetHandle = ('0x{0:x}' -f $windowHandle.ToInt64())
        RequestPosted = [bool]$closeRequested
        GracefulExitObserved = [bool]$gracefulExited
        ForcedCleanupUsed = [bool]$forcedCleanup
        ExitCode = $exitCode
        ExactIdentityStateAfterClose = $identityState
    }
    if (!$processExited -or $identityState -notin @('Absent', 'Reused')) {
        throw 'Candidate app retained process identity did not retire within bounded cleanup.'
    }
    if (!$closeRequested -or !$gracefulExited -or $forcedCleanup -or $exitCode -ne 0) {
        throw 'Candidate app did not exit successfully after normal launcher-root WM_CLOSE.'
    }
    $appOutput = Save-ProcessOutput $appRun $OutputDrainTimeoutSeconds
    $result.AppOutputDrainVerified = $appOutput.DrainVerified
    $result.AppStdout = $appOutput.Stdout
    $result.AppStderr = $appOutput.Stderr
    if (!$appOutput.DrainVerified) { throw 'Candidate app raw stdout/stderr did not drain completely within its bounded deadline.' }
    $process.Dispose()
    $appRun = $null

    $settingsAfter = Get-Content -Raw -LiteralPath $settingsPath -Encoding utf8 | ConvertFrom-Json -Depth 40
    if ($null -ne $settingsAfter.paste_sequence.next_hotkey -or $null -ne $settingsAfter.paste_sequence.cancel_hotkey) {
        throw 'Candidate app changed the isolated Paste Sequence shortcuts during its native smoke.'
    }
    $result.SettingsSha256After = (Get-FileHash -LiteralPath $settingsPath -Algorithm SHA256).Hash.ToLowerInvariant()
    $exeAfter = Get-FileBinding $exePath
    if ($exeAfter.LengthBytes -ne $result.Executable.LengthBytes -or $exeAfter.Sha256 -cne $ExpectedExecutableSha256.ToLowerInvariant()) {
        throw 'Candidate app artifact changed during native smoke.'
    }
    $result.ExecutableAfter = $exeAfter
    $result.Status = 'PASS'
}
catch {
    $result.Error = '{0}: {1}' -f $_.Exception.GetType().FullName, $_.Exception.Message
}
finally {
    if ($appRun) {
        $process = $appRun.Process
        try {
            $process.Refresh()
            if (!$process.HasExited) {
                $exactIdentity = [Math]::Abs(($process.StartTime.ToUniversalTime() - $appRun.StartedUtc).TotalMilliseconds) -le 5
                $cleanupCloseAttempt = [ordered]@{
                    Method = 'PostMessageW(WM_CLOSE) to revalidated launcher root HWND'
                    TargetHandle = $null
                    RequestPosted = $false
                    Error = $null
                }
                if ($exactIdentity) {
                    try {
                        $cleanupWindowSet = Get-ValidatedRootWindowSet $process.Id $expectedClientWidth $expectedClientHeight
                        if ($cleanupWindowSet.Roots.Count -eq 1) {
                            $cleanupRoot = $cleanupWindowSet.Roots[0]
                            $cleanupCloseAttempt.TargetHandle = '0x{0:x}' -f $cleanupRoot.Handle.ToInt64()
                            $cleanupCloseAttempt.RequestPosted = [TrackBNativeSmokeNative]::PostMessageW(
                                $cleanupRoot.Handle, [uint32]0x0010, [UIntPtr]::Zero, [IntPtr]::Zero)
                            $null = $process.WaitForExit([Math]::Min($GracefulCloseSeconds * 1000, 3000))
                        }
                        elseif ($cleanupWindowSet.Roots.Count -gt 1) {
                            $cleanupCloseAttempt.Error = 'Multiple launcher root windows matched; no cleanup close was sent.'
                        }
                    }
                    catch { $cleanupCloseAttempt.Error = $_.Exception.Message }
                }
                $result.CleanupCloseAttempt = [pscustomobject]$cleanupCloseAttempt
                $process.Refresh()
                if (!$process.HasExited -and $exactIdentity) {
                    $forcedCleanup = $true
                    $process.Kill($true)
                    $null = $process.WaitForExit($ForcedCleanupSeconds * 1000)
                }
            }
            $process.Refresh()
            if ($process.HasExited) {
                $result.FinalIdentityState = Get-ExactIdentityState $process.Id $appRun.StartedUtc
                try {
                    $appOutput = Save-ProcessOutput $appRun $OutputDrainTimeoutSeconds
                    $result.AppOutputDrainVerified = $appOutput.DrainVerified
                    $result.AppStdout = $appOutput.Stdout
                    $result.AppStderr = $appOutput.Stderr
                    if (!$appOutput.DrainVerified -and !$result.Error) {
                        $result.Error = 'Raw app stdout/stderr drain was incomplete within its bounded deadline.'
                    }
                }
                catch { if (!$result.Error) { $result.Error = 'Output drain failed: ' + $_.Exception.Message } }
            }
            else {
                $result.FinalIdentityState = 'Alive'
                if (!$result.Error) { $result.Error = 'Candidate app remained alive after exact-identity bounded cleanup.' }
            }
        }
        catch {
            $result.FinalIdentityState = 'Unknown'
            if (!$result.Error) { $result.Error = 'Candidate app cleanup failure: ' + $_.Exception.Message }
        }
        finally { $process.Dispose() }
    }
    if ($generatorRun) {
        try {
            if (!$generatorRun.Process.HasExited) {
                $generatorRun.Process.Kill($true)
                $null = $generatorRun.Process.WaitForExit(5000)
            }
            $generatorRun.Process.Refresh()
            $generatorCleanupState = if ($generatorRun.Process.HasExited) {
                Get-ExactIdentityState $generatorRun.Process.Id $generatorRun.StartedUtc
            } else { 'Alive' }
            if ($result.FixtureGeneration) {
                $result.FixtureGeneration.ExitCode = if ($generatorRun.Process.HasExited) { $generatorRun.Process.ExitCode } else { $null }
                $result.FixtureGeneration.CleanupIdentityState = $generatorCleanupState
                if ($generatorRun.Process.HasExited) {
                    try {
                        $generatorDrain = Save-ProcessOutput $generatorRun $OutputDrainTimeoutSeconds
                        $result.FixtureGeneration.OutputDrainVerified = $generatorDrain.DrainVerified
                        $result.FixtureGeneration.Output = $generatorDrain
                    }
                    catch {
                        if (!$result.FixtureGeneration.Output) {
                            $result.FixtureGeneration.Output = [pscustomobject][ordered]@{
                                DrainVerified = $false
                                DrainTimeoutSeconds = $OutputDrainTimeoutSeconds
                                Error = $_.Exception.Message
                            }
                        }
                    }
                }
            }
            $generatorRun.Process.Dispose()
        }
        catch { if (!$result.Error) { $result.Error = 'Fixture generator cleanup failure: ' + $_.Exception.Message } }
    }
    $result.EndedUtc = [DateTimeOffset]::UtcNow.ToString('o')
}

$json = ConvertTo-Json -InputObject $result -Depth 10
try {
    [IO.File]::WriteAllText($resultPath, $json + [Environment]::NewLine, [Text.UTF8Encoding]::new($false))
}
catch {
    Write-Error "Could not persist native smoke result at '$resultPath': $($_.Exception.Message)"
    exit 1
}
Write-Output $json
if ($result.Status -cne 'PASS') { exit 1 }
