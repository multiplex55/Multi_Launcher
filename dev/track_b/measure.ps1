#requires -Version 7.0
<#
.SYNOPSIS
Runs one native executable and writes a unique Track B measurement record.

.DESCRIPTION
Arguments are passed as a .NET ArgumentList through the checked owned-process
runner; no command string is evaluated. The run has a finite stage deadline and
records cleanup, identity and output-capture outcomes with the measurement.
The script returns a record object and never exits the calling PowerShell host.
Callers must inspect .Succeeded / .ExitCode. Environment overrides and the
PowerShell location are restored before the result is returned.

.EXAMPLE
$run = & .\dev\track_b\measure.ps1 -Executable cargo.exe `
    -ArgumentList @('build', '--release', '--bin', 'multi_launcher', '--timings') `
    -ScenarioId 'edited-release-1' -Profile 'release' -CacheState warmed
if (!$run.Succeeded) { throw "Build failed with exit $($run.ExitCode)" }
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$Executable,
    [string[]]$ArgumentList = @(),
    [Parameter(Mandatory = $true)][string]$ScenarioId,
    [string]$EvidenceDirectory = (Join-Path ([IO.Path]::GetTempPath()) 'Multi_Launcher\track-b'),
    [string]$WorkingDirectory = (Get-Location).Path,
    [string]$SourceRoot,
    [string]$BaselineSha,
    [string]$Profile = 'unspecified',
    [ValidateSet('unknown', 'warmed', 'clean', 'branch-switched', 'new-profile')]
    [string]$CacheState = 'unknown',
    [string]$TargetDirectory,
    [string]$PreviousCommand,
    [ValidateRange(-1, 1000000)][int]$SourceFilesModified = -1,
    [string[]]$FingerprintFlags = @(),
    [string[]]$TargetFlags = @(),
    [hashtable]$EnvironmentOverrides = @{},
    [hashtable]$FixtureMetadata = @{},
    [string[]]$FixturePaths = @(),
    [string]$CargoTimingsPath,
    [ValidateSet('unknown', 'yes', 'no')][string]$CompilationObserved = 'unknown',
    [ValidateSet('unknown', 'yes', 'no')][string]$BinaryLinkObserved = 'unknown',
    [ValidateSet('unknown', 'fresh', 'not-fresh')][string]$CargoFreshness = 'unknown',
    [string]$WorkEvidenceNote,
    [ValidateSet('unspecified', 'compile', 'test', 'native', 'provenance')][string]$Stage = 'unspecified',
    [ValidateRange(1, 86400)][int]$TimeoutSeconds = 1800,
    [ValidateRange(1, 60)][int]$CleanupTimeoutSeconds = 5
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot '..\workflows\owned_process.ps1')

function Test-PathWithinDirectory {
    param([string]$Candidate, [string]$Directory)
    $candidatePath = [IO.Path]::GetFullPath($Candidate).TrimEnd([char[]]@('\', '/'))
    $directoryPath = [IO.Path]::GetFullPath($Directory).TrimEnd([char[]]@('\', '/'))
    if ([string]::IsNullOrEmpty($directoryPath)) { $directoryPath = [IO.Path]::GetPathRoot([IO.Path]::GetFullPath($Directory)) }
    $prefix = $directoryPath + [IO.Path]::DirectorySeparatorChar
    return $candidatePath.Equals($directoryPath, [StringComparison]::OrdinalIgnoreCase) -or
        $candidatePath.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase)
}

function Resolve-NativeExecutable {
    param([Parameter(Mandatory = $true)][string]$Name)
    $application = Get-Command -Name $Name -CommandType Application -ErrorAction Stop | Select-Object -First 1
    if ($null -eq $application) { throw "Executable was not found as a native application: $Name" }
    $path = if ($application.Source) { $application.Source } else { $application.Path }
    if ([IO.Path]::GetExtension($path) -in @('.ps1', '.cmd', '.bat')) {
        throw "Pass a native executable, not a shell script or command wrapper: $path"
    }
    return [IO.Path]::GetFullPath($path)
}

function Invoke-TextProcess {
    param(
        [string]$ExecutablePath,
        [string[]]$Arguments,
        [string]$WorkingDirectory = (Get-Location).Path,
        [ValidateRange(1, 120)][int]$TimeoutSeconds = 15
    )
    $helperDirectory = Join-Path ([IO.Path]::GetTempPath()) ('measure-provenance-' + [guid]::NewGuid().ToString('N'))
    [IO.Directory]::CreateDirectory($helperDirectory) | Out-Null
    $stdoutPath = Join-Path $helperDirectory 'stdout.log'
    $stderrPath = Join-Path $helperDirectory 'stderr.log'
    try {
        $result = Invoke-CheckedOwnedProcess -Executable $ExecutablePath -ArgumentList @($Arguments) `
            -WorkingDirectory $WorkingDirectory -Stage 'provenance' -TimeoutSeconds $TimeoutSeconds `
            -CleanupTimeoutSeconds 3 -StdoutPath $stdoutPath -StderrPath $stderrPath
        $stdout = if (Test-Path -LiteralPath $stdoutPath -PathType Leaf) { [IO.File]::ReadAllText($stdoutPath) } else { '' }
        $stderr = if (Test-Path -LiteralPath $stderrPath -PathType Leaf) { [IO.File]::ReadAllText($stderrPath) } else { '' }
        if (!$result.Succeeded) {
            throw "Provenance command failed or could not be verified ($($result.Reason), exit $($result.ExitCode)): $ExecutablePath"
        }
        return [pscustomobject]@{ ExitCode = $result.ExitCode; Stdout = $stdout; Stderr = $stderr }
    }
    finally {
        try { [IO.Directory]::Delete($helperDirectory, $true) } catch { }
    }
}

function Invoke-GitText {
    param([string]$GitPath, [string]$Repository, [string[]]$GitArguments)
    $arguments = @('-C', $Repository) + $GitArguments
    $result = Invoke-TextProcess -ExecutablePath $GitPath -Arguments $arguments -WorkingDirectory $Repository
    if ($result.ExitCode -ne 0) { throw "git $($GitArguments -join ' ') failed: $($result.Stderr.Trim())" }
    return $result.Stdout.TrimEnd("`r", "`n")
}

function Resolve-InputPath {
    param([string]$Path, [string]$BaseDirectory)
    if ([IO.Path]::IsPathRooted($Path)) { return [IO.Path]::GetFullPath($Path) }
    return [IO.Path]::GetFullPath((Join-Path $BaseDirectory $Path))
}

function Get-FileSha256 {
    param([string]$Path)
    return (Get-FileHash -LiteralPath $Path -Algorithm SHA256 -ErrorAction Stop).Hash.ToLowerInvariant()
}

if ([string]::IsNullOrWhiteSpace($ScenarioId)) { throw 'ScenarioId must not be empty.' }
if ($null -eq $ArgumentList) { $ArgumentList = @() }
if ($null -eq $EnvironmentOverrides) { $EnvironmentOverrides = @{} }
if ($null -eq $FixtureMetadata) { $FixtureMetadata = @{} }

$resolvedExecutable = Resolve-NativeExecutable -Name $Executable
$workingPath = (Resolve-Path -LiteralPath $WorkingDirectory -ErrorAction Stop).ProviderPath
if ([string]::IsNullOrWhiteSpace($SourceRoot)) {
    $SourceRoot = Join-Path $PSScriptRoot '..\..'
}
$sourceRootPath = (Resolve-Path -LiteralPath $SourceRoot -ErrorAction Stop).ProviderPath
$gitApplication = Get-Command -Name git.exe -CommandType Application -ErrorAction Stop | Select-Object -First 1
$gitPath = if ($gitApplication.Source) { $gitApplication.Source } else { $gitApplication.Path }
$gitRoot = Invoke-GitText -GitPath $gitPath -Repository $sourceRootPath -GitArguments @('rev-parse', '--show-toplevel')
$gitRootPath = (Resolve-Path -LiteralPath $gitRoot -ErrorAction Stop).ProviderPath
$sourceSha = Invoke-GitText -GitPath $gitPath -Repository $gitRootPath -GitArguments @('rev-parse', 'HEAD')
$dirtyText = Invoke-GitText -GitPath $gitPath -Repository $gitRootPath -GitArguments @('status', '--short', '--untracked-files=all')
$dirtyFiles = @($dirtyText -split '\r?\n' | Where-Object { $_.Length -gt 0 })

$evidenceRoot = [IO.Path]::GetFullPath($EvidenceDirectory)
if (Test-PathWithinDirectory -Candidate $evidenceRoot -Directory $gitRootPath) {
    throw "EvidenceDirectory must be outside the source worktree: $evidenceRoot"
}
[IO.Directory]::CreateDirectory($evidenceRoot) | Out-Null
$scenarioSlug = [regex]::Replace($ScenarioId, '[^A-Za-z0-9_-]+', '_').Trim('_')
if ([string]::IsNullOrEmpty($scenarioSlug)) { $scenarioSlug = 'scenario' }
$runDirectory = $null
for ($attempt = 0; $attempt -lt 5 -and $null -eq $runDirectory; $attempt++) {
    $stamp = [DateTimeOffset]::UtcNow.ToString('yyyyMMddTHHmmssfffZ')
    $candidate = Join-Path $evidenceRoot ('{0}_{1}_{2}' -f $scenarioSlug, $stamp, [guid]::NewGuid().ToString('N'))
    if (Test-PathWithinDirectory -Candidate $candidate -Directory $gitRootPath) {
        throw "Evidence run path resolved inside the source worktree: $candidate"
    }
    try {
        New-Item -ItemType Directory -Path $candidate -ErrorAction Stop | Out-Null
        $runDirectory = $candidate
    }
    catch {
        if (!(Test-Path -LiteralPath $candidate)) { throw }
    }
}
if ($null -eq $runDirectory) { throw 'Could not allocate a unique evidence directory.' }

$stdoutPath = Join-Path $runDirectory 'stdout.bin'
$stderrPath = Join-Path $runDirectory 'stderr.bin'
$manifestPath = Join-Path $runDirectory 'measurement.json'
$timingPath = $null
$defaultTargetDirectory = Join-Path $workingPath 'target'
$cliTargetDirectory = $null
for ($index = 0; $index -lt $ArgumentList.Count; $index++) {
    $argument = [string]$ArgumentList[$index]
    if ($argument -eq '--target-dir') {
        if ($index + 1 -ge $ArgumentList.Count -or [string]::IsNullOrWhiteSpace([string]$ArgumentList[$index + 1])) {
            throw 'Cargo --target-dir requires a path argument.'
        }
        $cliTargetDirectory = [string]$ArgumentList[$index + 1]
        break
    }
    if ($argument.StartsWith('--target-dir=', [StringComparison]::Ordinal)) {
        $cliTargetDirectory = $argument.Substring('--target-dir='.Length)
        if ([string]::IsNullOrWhiteSpace($cliTargetDirectory)) { throw 'Cargo --target-dir requires a path argument.' }
        break
    }
}
$environmentTargetSpecified = $false
$environmentTargetValue = $null
foreach ($name in $EnvironmentOverrides.Keys) {
    if ([string]$name -ieq 'CARGO_TARGET_DIR') {
        $environmentTargetSpecified = $true
        $environmentTargetValue = $EnvironmentOverrides[$name]
        break
    }
}
$explicitTargetDirectory = ![string]::IsNullOrWhiteSpace($TargetDirectory)
$targetDirectorySource = 'default'
if ($explicitTargetDirectory) {
    $effectiveTargetDirectory = Resolve-InputPath -Path $TargetDirectory -BaseDirectory $workingPath
    $targetDirectorySource = 'parameter'
    if ($cliTargetDirectory) {
        $cliTargetPath = Resolve-InputPath -Path $cliTargetDirectory -BaseDirectory $workingPath
        if (!$effectiveTargetDirectory.Equals($cliTargetPath, [StringComparison]::OrdinalIgnoreCase)) {
            throw 'TargetDirectory does not match the command line --target-dir value.'
        }
    }
    if (!$cliTargetDirectory) {
        if ($environmentTargetSpecified) {
            $environmentTargetPath = if ($null -eq $environmentTargetValue -or [string]::IsNullOrWhiteSpace([string]$environmentTargetValue)) {
                [IO.Path]::GetFullPath($defaultTargetDirectory)
            }
            else {
                Resolve-InputPath -Path ([string]$environmentTargetValue) -BaseDirectory $workingPath
            }
            if (!$effectiveTargetDirectory.Equals($environmentTargetPath, [StringComparison]::OrdinalIgnoreCase)) {
                throw 'TargetDirectory does not match the CARGO_TARGET_DIR environment override.'
            }
        }
        elseif (![string]::IsNullOrWhiteSpace($env:CARGO_TARGET_DIR)) {
            $callerTargetPath = Resolve-InputPath -Path $env:CARGO_TARGET_DIR -BaseDirectory $workingPath
            if (!$effectiveTargetDirectory.Equals($callerTargetPath, [StringComparison]::OrdinalIgnoreCase)) {
                throw 'TargetDirectory does not match the caller CARGO_TARGET_DIR value.'
            }
        }
    }
}
elseif ($cliTargetDirectory) {
    $effectiveTargetDirectory = Resolve-InputPath -Path $cliTargetDirectory -BaseDirectory $workingPath
    $targetDirectorySource = 'command-line'
}
elseif ($environmentTargetSpecified) {
    if ($null -eq $environmentTargetValue -or [string]::IsNullOrWhiteSpace([string]$environmentTargetValue)) {
        $effectiveTargetDirectory = [IO.Path]::GetFullPath($defaultTargetDirectory)
    }
    else {
        $effectiveTargetDirectory = Resolve-InputPath -Path ([string]$environmentTargetValue) -BaseDirectory $workingPath
    }
    $targetDirectorySource = 'environment-override'
}
elseif (![string]::IsNullOrWhiteSpace($env:CARGO_TARGET_DIR)) {
    $effectiveTargetDirectory = Resolve-InputPath -Path $env:CARGO_TARGET_DIR -BaseDirectory $workingPath
    $targetDirectorySource = 'caller-environment'
}
else {
    $effectiveTargetDirectory = [IO.Path]::GetFullPath($defaultTargetDirectory)
}
if (![string]::IsNullOrWhiteSpace($CargoTimingsPath)) {
    $timingPath = Resolve-InputPath -Path $CargoTimingsPath -BaseDirectory $workingPath
}
elseif ([IO.Path]::GetFileName($resolvedExecutable) -ieq 'cargo.exe' -and $ArgumentList -contains '--timings') {
    $timingPath = Join-Path $effectiveTargetDirectory 'cargo-timings\cargo-timing.html'
}

$timingBeforeExists = $false
$timingBeforeHash = $null
$timingBeforeWriteUtc = $null
if ($timingPath -and (Test-Path -LiteralPath $timingPath -PathType Leaf)) {
    try {
        $timingBeforeExists = $true
        $timingBeforeHash = Get-FileSha256 -Path $timingPath
        $timingBeforeWriteUtc = (Get-Item -LiteralPath $timingPath).LastWriteTimeUtc
    }
    catch {
        $timingBeforeExists = $true
        $timingBeforeHash = $null
    }
}

$fixtureRecords = @(
    foreach ($fixture in $FixturePaths) {
        $fixturePath = Resolve-InputPath -Path $fixture -BaseDirectory $workingPath
        if (!(Test-Path -LiteralPath $fixturePath -PathType Leaf)) { throw "Fixture file does not exist: $fixture" }
        [pscustomobject]@{
            SuppliedPath = $fixture
            ResolvedPath = $fixturePath
            LengthBytes = (Get-Item -LiteralPath $fixturePath).Length
            Sha256 = Get-FileSha256 -Path $fixturePath
        }
    }
)

$environmentNames = @($EnvironmentOverrides.Keys | ForEach-Object { [string]$_ } | Sort-Object)
$environmentBefore = [ordered]@{}
foreach ($name in $environmentNames) {
    if ([string]::IsNullOrWhiteSpace($name) -or $name.Contains('=') -or $name.Contains([char]0)) {
        throw "Invalid environment variable name: $name"
    }
    $environmentBefore[$name] = [Environment]::GetEnvironmentVariable($name, 'Process')
}

$ownedRun = $null
$locationPushed = $false
$processStarted = $false
$executionCompleted = $false
$nativeExitCode = $null
$runnerError = $null
$restorationErrors = [System.Collections.Generic.List[string]]::new()
$recordingErrors = [System.Collections.Generic.List[string]]::new()
$watch = $null
$startedUtc = $null
$endedUtc = $null

try {
    Push-Location -LiteralPath $workingPath
    $locationPushed = $true
    foreach ($name in $environmentNames) {
        $value = $EnvironmentOverrides[$name]
        if ($null -eq $value) {
            [Environment]::SetEnvironmentVariable($name, $null, 'Process')
        }
        else {
            [Environment]::SetEnvironmentVariable($name, [string]$value, 'Process')
        }
    }

    $startedUtc = [DateTimeOffset]::UtcNow
    $watch = [Diagnostics.Stopwatch]::StartNew()
    $ownedRun = Invoke-CheckedOwnedProcess -Executable $resolvedExecutable -ArgumentList @($ArgumentList) `
        -WorkingDirectory $workingPath -Stage $Stage -TimeoutSeconds $TimeoutSeconds `
        -CleanupTimeoutSeconds $CleanupTimeoutSeconds -StdoutPath $stdoutPath -StderrPath $stderrPath
    $processStarted = $ownedRun.ProcessStarted
    $executionCompleted = $ownedRun.ProcessExited -and $ownedRun.CleanupVerified -and $ownedRun.OutputDrainVerified
    $nativeExitCode = $ownedRun.ExitCode
    foreach ($errorText in $ownedRun.RecordingErrors) { $recordingErrors.Add([string]$errorText) }
    foreach ($errorText in $ownedRun.CleanupErrors) { $recordingErrors.Add([string]$errorText) }
    if ($ownedRun.Reason -notin @('completed', 'failed exit')) { $runnerError = [string]$ownedRun.Reason }
}
catch {
    $runnerError = '{0}: {1}' -f $_.Exception.GetType().FullName, $_.Exception.Message
}
finally {
    if ($watch) {
        $watch.Stop()
        $endedUtc = [DateTimeOffset]::UtcNow
    }
    foreach ($name in $environmentNames) {
        try { [Environment]::SetEnvironmentVariable($name, $environmentBefore[$name], 'Process') }
        catch { $restorationErrors.Add(('Environment restore {0}: {1}' -f $name, $_.Exception.Message)) }
    }
    if ($locationPushed) {
        try { Pop-Location }
        catch { $restorationErrors.Add(('Location restore: {0}' -f $_.Exception.Message)) }
    }
}

$timingDisposition = if (!$timingPath) { 'not-configured' } else { 'not-created' }
$timingSnapshotPath = $null
$timingAfterHash = $null
$timingAfterWriteUtc = $null
$timingError = $null
if ($timingPath) {
    try {
        if (Test-Path -LiteralPath $timingPath -PathType Leaf) {
            $timingAfterHash = Get-FileSha256 -Path $timingPath
            $timingAfterWriteUtc = (Get-Item -LiteralPath $timingPath).LastWriteTimeUtc
            $wasUpdated = !$timingBeforeExists -or
                ($null -ne $timingBeforeHash -and $timingBeforeHash -ne $timingAfterHash) -or
                ($null -ne $timingBeforeWriteUtc -and $timingAfterWriteUtc -gt $timingBeforeWriteUtc)
            if ($wasUpdated) {
                $timingSnapshotPath = Join-Path $runDirectory 'cargo-timing.html'
                Copy-Item -LiteralPath $timingPath -Destination $timingSnapshotPath -ErrorAction Stop
                $timingDisposition = 'captured-new-or-updated'
            }
            else {
                $timingDisposition = 'unchanged-not-copied'
            }
        }
        elseif ($timingBeforeExists) {
            $timingDisposition = 'missing-after-run'
        }
    }
    catch {
        $timingDisposition = 'capture-error'
        $timingError = '{0}: {1}' -f $_.Exception.GetType().FullName, $_.Exception.Message
        $timingSnapshotPath = $null
    }
}

$wallSeconds = if ($watch) { [Math]::Round($watch.Elapsed.TotalSeconds, 6) } else { $null }
$succeeded = $processStarted -and $executionCompleted -and $null -eq $runnerError -and $null -ne $ownedRun -and
    $ownedRun.Succeeded -and $nativeExitCode -eq 0 -and $restorationErrors.Count -eq 0 -and $recordingErrors.Count -eq 0
$targetDirectoryIsExplicit = ![string]::IsNullOrWhiteSpace($TargetDirectory)
$record = [pscustomobject][ordered]@{
    ScenarioId = $ScenarioId
    StartedUtc = if ($startedUtc) { $startedUtc.ToString('o') } else { $null }
    EndedUtc = if ($endedUtc) { $endedUtc.ToString('o') } else { $null }
    WallSeconds = $wallSeconds
    Stage = $Stage
    DeadlineUtc = if ($ownedRun) { $ownedRun.DeadlineUtc } else { $null }
    TimeoutSeconds = $TimeoutSeconds
    TimeoutReason = if ($ownedRun) { $ownedRun.TimeoutReason } else { $null }
    TimedOut = if ($ownedRun) { $ownedRun.TimedOut } else { $false }
    TimingMeaning = 'Native process wall time; includes process startup and exit wait.'
    LinkerDuration = 'NOT_MEASURED_BY_PROCESS_WALL_TIME'
    ExecutableInput = $Executable
    Executable = $resolvedExecutable
    ArgumentList = @($ArgumentList)
    WorkingDirectory = $workingPath
    EvidenceDirectory = $runDirectory
    StdoutPath = $stdoutPath
    StderrPath = $stderrPath
    ManifestPath = $manifestPath
    ExitCode = $nativeExitCode
    ProcessStarted = $processStarted
    ExecutionCompleted = $executionCompleted
    Succeeded = $succeeded
    RunnerError = $runnerError
    ExecutionReason = if ($ownedRun) { $ownedRun.Reason } else { $runnerError }
    CleanupVerified = if ($ownedRun) { $ownedRun.CleanupVerified } else { $false }
    CleanupErrors = if ($ownedRun) { @($ownedRun.CleanupErrors) } else { @() }
    OutputDrainVerified = if ($ownedRun) { $ownedRun.OutputDrainVerified } else { $false }
    OwnedProcesses = if ($ownedRun) { @($ownedRun.OwnedProcesses) } else { @() }
    PostCleanup = if ($ownedRun) { @($ownedRun.PostCleanup) } else { @() }
    OutputBytes = if ($ownedRun) { [pscustomobject]@{ StdoutRead = $ownedRun.StdoutBytesRead; StdoutRecorded = $ownedRun.StdoutBytesRecorded; StderrRead = $ownedRun.StderrBytesRead; StderrRecorded = $ownedRun.StderrBytesRecorded } } else { $null }
    RecordingErrors = @($recordingErrors)
    RestorationErrors = @($restorationErrors)
    SourceRoot = $gitRootPath
    SourceSha = $sourceSha
    DirtyFiles = @($dirtyFiles)
    BaselineSha = if ([string]::IsNullOrWhiteSpace($BaselineSha)) { $null } else { $BaselineSha }
    Profile = $Profile
    CacheState = $CacheState
    TargetDirectory = $effectiveTargetDirectory
    TargetDirectorySource = $targetDirectorySource
    PreviousCommand = $PreviousCommand
    SourceFilesModified = if ($SourceFilesModified -lt 0) { $null } else { $SourceFilesModified }
    FingerprintFlags = @($FingerprintFlags)
    TargetFlags = @($TargetFlags)
    EnvironmentOverrideNames = @($environmentNames)
    EnvironmentOverrideValuesRecorded = $false
    FixtureMetadata = $FixtureMetadata
    Fixtures = @($fixtureRecords)
    CompilationObserved = $CompilationObserved
    BinaryLinkObserved = $BinaryLinkObserved
    CargoFreshness = $CargoFreshness
    WorkEvidenceNote = $WorkEvidenceNote
    CargoTimings = [pscustomobject][ordered]@{
        ReportPath = $timingPath
        Disposition = $timingDisposition
        ReportSha256Before = $timingBeforeHash
        ReportSha256After = $timingAfterHash
        ReportWriteTimeUtcBefore = if ($timingBeforeWriteUtc) { $timingBeforeWriteUtc.ToString('o') } else { $null }
        ReportWriteTimeUtcAfter = if ($timingAfterWriteUtc) { $timingAfterWriteUtc.ToString('o') } else { $null }
        SnapshotPath = $timingSnapshotPath
        Error = $timingError
    }
}

try {
    $json = ConvertTo-Json -InputObject $record -Depth 16 -ErrorAction Stop
    [IO.File]::WriteAllText($manifestPath, $json + [Environment]::NewLine, [Text.UTF8Encoding]::new($false))
}
catch {
    $recordingErrors.Add(('Measurement manifest: {0}' -f $_.Exception.Message))
    $record.Succeeded = $false
    $record.RecordingErrors = @($recordingErrors)
    if ($record.ExecutionReason -eq 'completed') { $record.ExecutionReason = 'measurement recording failure' }
}
return $record
