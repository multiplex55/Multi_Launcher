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
    [ValidateSet('unknown', 'no-op', 'edited-source', 'preparation', 'cold')][string]$WorkKind = 'unknown',
    [string]$ControlledChangeId,
    [string[]]$ControlledChangePaths = @(),
    [hashtable]$ComparisonSelection = @{},
    [ValidateSet('unspecified', 'compile', 'test', 'native', 'provenance')][string]$Stage = 'unspecified',
    [ValidateRange(1, 86400)][int]$TimeoutSeconds = 1800,
    [ValidateRange(1, 60)][int]$CleanupTimeoutSeconds = 5,
    [ValidateSet('none', 'verified-msvc-vctip')][string]$AuxiliaryProcessPolicy = 'none'
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot '..\workflows\owned_process.ps1')
. (Join-Path $PSScriptRoot 'comparison.ps1')

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

function Test-ProcessEnvironmentVariablePresent {
    param([Parameter(Mandatory = $true)][string]$Name)
    return [Environment]::GetEnvironmentVariables([EnvironmentVariableTarget]::Process).Contains($Name)
}

function Remove-ProcessEnvironmentVariable {
    param([Parameter(Mandatory = $true)][string]$Name)
    if (Test-ProcessEnvironmentVariablePresent -Name $Name) {
        Remove-Item -LiteralPath ('Env:{0}' -f $Name) -ErrorAction Stop
    }
    if (Test-ProcessEnvironmentVariablePresent -Name $Name) {
        throw "Environment variable remained present after removal: $Name"
    }
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

function Resolve-RustcForComparison {
    $configured = [Environment]::GetEnvironmentVariable('RUSTC', 'Process')
    if (![string]::IsNullOrWhiteSpace($configured)) {
        $candidate = $configured.Trim()
        if ($candidate.Length -ge 2 -and $candidate[0] -eq '"' -and $candidate[$candidate.Length - 1] -eq '"') {
            $candidate = $candidate.Substring(1, $candidate.Length - 2)
        }
        if (Test-Path -LiteralPath $candidate -PathType Leaf) { return [IO.Path]::GetFullPath($candidate) }
        try {
            $command = Get-Command -Name $candidate -CommandType Application -ErrorAction Stop | Select-Object -First 1
            if ($command) { return [IO.Path]::GetFullPath($(if ($command.Source) { $command.Source } else { $command.Path })) }
        }
        catch { return $null }
        return $null
    }
    try {
        $command = Get-Command -Name 'rustc.exe' -CommandType Application -ErrorAction Stop | Select-Object -First 1
        if ($command) { return [IO.Path]::GetFullPath($(if ($command.Source) { $command.Source } else { $command.Path })) }
    }
    catch { return $null }
    return $null
}

function Get-InstalledMsvcVctipCandidates {
    $programFiles = [Environment]::GetFolderPath('ProgramFiles')
    if ([string]::IsNullOrWhiteSpace($programFiles)) { throw 'The Program Files location is unavailable for MSVC trust preflight.' }
    $visualStudioRoot = Join-Path $programFiles 'Microsoft Visual Studio'
    if (!(Test-Path -LiteralPath $visualStudioRoot -PathType Container)) { throw "Installed Visual Studio root was not found: $visualStudioRoot" }
    $candidates = [System.Collections.Generic.List[string]]::new()
    foreach ($year in Get-ChildItem -LiteralPath $visualStudioRoot -Directory -ErrorAction Stop) {
        if ($year.Name -notmatch '^20[0-9]{2}$') { continue }
        foreach ($edition in Get-ChildItem -LiteralPath $year.FullName -Directory -ErrorAction Stop) {
            if ($edition.Name -notin @('Community', 'Professional', 'Enterprise', 'BuildTools')) { continue }
            $toolRoot = Join-Path $edition.FullName 'VC\Tools\MSVC'
            if (!(Test-Path -LiteralPath $toolRoot -PathType Container)) { continue }
            foreach ($version in Get-ChildItem -LiteralPath $toolRoot -Directory -ErrorAction Stop) {
                if ($version.Name -notmatch '^[0-9]+\.[0-9]+\.[0-9]+$') { continue }
                $candidate = Join-Path $version.FullName 'bin\Hostx64\x64\vctip.exe'
                if (Test-Path -LiteralPath $candidate -PathType Leaf) { $candidates.Add([IO.Path]::GetFullPath($candidate)) }
            }
        }
    }
    $paths = @($candidates | Sort-Object -Unique)
    if ($paths.Count -eq 0) { throw 'No installed x64 MSVC VCTIP executable was found under Program Files.' }
    if ($paths.Count -gt 16) { throw 'The installed MSVC VCTIP candidate count exceeded its bounded trust-preflight limit.' }
    return $paths
}

function Invoke-VerifiedMsvcVctipPreflight {
    param([Parameter(Mandatory)][string]$RunDirectory)
    $preflightWatch = [Diagnostics.Stopwatch]::StartNew()
    $leases = [System.Collections.Generic.List[object]]::new()
    $record = [ordered]@{
        Policy = 'verified-msvc-vctip'
        Status = 'FAIL'
        Command = $null
        TimeoutSeconds = 20
        ElapsedSeconds = $null
        Candidates = @()
        Measurement = $null
        LeaseCleanupErrors = @()
        Error = $null
    }
    try {
        $paths = @(Get-InstalledMsvcVctipCandidates)
        foreach ($path in $paths) {
            # FileShare.Read denies any existing or future write/delete handle;
            # keep this lease open through the Cargo invocation and Job cleanup.
            $lease = $null
            try {
                $lease = [IO.File]::Open($path, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
                $identity = [MultiLauncher.Dev.OwnedProcessNative]::GetOpenFileDescriptor($lease.SafeFileHandle)
                $leases.Add([pscustomobject]@{ Path = $path; Lease = $lease; InitialIdentity = $identity })
                $lease = $null
            }
            finally { if ($lease) { $lease.Dispose() } }
        }

        $powershell = Get-Command -Name 'pwsh.exe' -CommandType Application -ErrorAction Stop | Select-Object -First 1
        $powershellPath = if ($powershell.Source) { $powershell.Source } else { $powershell.Path }
        $verifierPath = Join-Path $PSScriptRoot '..\workflows\verify_msvc_auxiliary.ps1'
        $verifierPath = (Resolve-Path -LiteralPath $verifierPath -ErrorAction Stop).ProviderPath
        $arguments = @('-NoLogo', '-NoProfile', '-NonInteractive', '-File', $verifierPath, '-Paths') + $paths
        $record.Command = [pscustomobject][ordered]@{ Executable = [IO.Path]::GetFullPath($powershellPath); ArgumentList = @($arguments); WorkingDirectory = $workingPath }
        $stdoutPath = Join-Path $RunDirectory 'msvc-trust-preflight.stdout.json'
        $stderrPath = Join-Path $RunDirectory 'msvc-trust-preflight.stderr.log'
        $verified = Invoke-CheckedOwnedProcess -Executable $powershellPath -ArgumentList $arguments `
            -WorkingDirectory $workingPath -Stage 'provenance' -TimeoutSeconds 20 -CleanupTimeoutSeconds 5 `
            -StdoutPath $stdoutPath -StderrPath $stderrPath
        $record.Measurement = [pscustomobject][ordered]@{
            Succeeded = [bool]$verified.Succeeded
            Reason = [string]$verified.Reason
            ExitCode = $verified.ExitCode
            TimedOut = [bool]$verified.TimedOut
            CleanupVerified = [bool]$verified.CleanupVerified
            OutputDrainVerified = [bool]$verified.OutputDrainVerified
            RecordingErrors = @($verified.RecordingErrors)
            CleanupErrors = @($verified.CleanupErrors)
            OwnedProcesses = @($verified.OwnedProcesses)
            PostCleanup = @($verified.PostCleanup)
            StdoutPath = $stdoutPath
            StderrPath = $stderrPath
        }
        $output = if (Test-Path -LiteralPath $stdoutPath -PathType Leaf) { [IO.File]::ReadAllText($stdoutPath) } else { '' }
        $verification = if (![string]::IsNullOrWhiteSpace($output)) { $output | ConvertFrom-Json -AsHashtable -ErrorAction Stop } else { $null }
        if ($verification.SchemaVersion -ne 1 -or $verification.Policy -cne 'verified-msvc-vctip') {
            throw 'The bounded trust verifier returned a missing or unsupported receipt.'
        }
        $verifiedDescriptors = [System.Collections.Generic.List[object]]::new()
        foreach ($candidate in @($verification.Candidates)) {
            $leaseEntry = @($leases | Where-Object { [string]$_.Path -ieq [string]$candidate.Path })
            if ($leaseEntry.Count -ne 1) { throw "Trust receipt did not map uniquely to its held candidate: $($candidate.Path)" }
            $leaseEntry = $leaseEntry[0]
            $heldIdentity = [MultiLauncher.Dev.OwnedProcessNative]::GetOpenFileDescriptor($leaseEntry.Lease.SafeFileHandle)
            if ([string]$heldIdentity.FileId -cne [string]$candidate.FileId -or
                [string]$heldIdentity.CanonicalPath -ine [string]$candidate.CanonicalPath -or
                [string]$leaseEntry.InitialIdentity.FileId -cne [string]$heldIdentity.FileId) {
                throw "Trusted MSVC image changed file identity during bounded preflight: $($candidate.Path)"
            }
            if ($candidate.ValidMicrosoftSignature -and [string]$candidate.SignatureStatus -ceq 'Valid' -and
                [string]$candidate.SignerOrganization -ceq 'Microsoft Corporation' -and
                [string]$candidate.SignerThumbprint -match '^[0-9A-Fa-f]{40,64}$' -and
                [string]$candidate.SignerCertificateSha256 -match '^[0-9a-f]{64}$') {
                $descriptor = [pscustomobject][ordered]@{
                    CanonicalPath = [string]$candidate.CanonicalPath
                    FileId = [string]$candidate.FileId
                    Sha256 = [string]$candidate.Sha256
                    SignatureStatus = [string]$candidate.SignatureStatus
                    SignerThumbprint = [string]$candidate.SignerThumbprint
                    SignerCertificateSha256 = [string]$candidate.SignerCertificateSha256
                    SignerOrganization = [string]$candidate.SignerOrganization
                    SignerSubject = [string]$candidate.SignerSubject
                    VerificationSource = [string]$candidate.VerificationSource
                }
                $verifiedDescriptors.Add([pscustomobject]@{ Descriptor = $descriptor; Lease = $leaseEntry.Lease })
            }
        }
        $record.Candidates = @($verification.Candidates | ForEach-Object {
            [pscustomobject][ordered]@{
                CanonicalPath = $_.CanonicalPath; FileId = $_.FileId; Sha256 = $_.Sha256
                SignatureStatus = $_.SignatureStatus; SignerThumbprint = $_.SignerThumbprint
                SignerCertificateSha256 = $_.SignerCertificateSha256; SignerOrganization = $_.SignerOrganization
                ValidMicrosoftSignature = [bool]$_.ValidMicrosoftSignature; Error = $_.Error
            }
        })
        if (!$verified.Succeeded -or $verifiedDescriptors.Count -eq 0) {
            throw 'No candidate had a bounded, valid Microsoft Authenticode identity and stable held-file identity.'
        }
        $record.Status = 'PASS'
        $record.Descriptors = @($verifiedDescriptors | ForEach-Object { $_.Descriptor })
    }
    catch {
        $record.Error = '{0}: {1}' -f $_.Exception.GetType().FullName, $_.Exception.Message
        $leaseErrors = [System.Collections.Generic.List[string]]::new()
        foreach ($entry in $leases) { try { $entry.Lease.Dispose() } catch { $leaseErrors.Add($_.Exception.Message) } }
        $record.LeaseCleanupErrors = @($leaseErrors)
        $leases.Clear()
        $record.Status = 'FAIL'
    }
    finally {
        $preflightWatch.Stop()
        $record.ElapsedSeconds = [Math]::Round($preflightWatch.Elapsed.TotalSeconds, 6)
    }
    return [pscustomobject]@{
        Receipt = [pscustomobject]$record
        Images = if ($record.Status -eq 'PASS') { @($verifiedDescriptors) } else { @() }
        Leases = @($leases | ForEach-Object { $_.Lease })
    }
}

if ([string]::IsNullOrWhiteSpace($ScenarioId)) { throw 'ScenarioId must not be empty.' }
if ($null -eq $ArgumentList) { $ArgumentList = @() }
if ($null -eq $EnvironmentOverrides) { $EnvironmentOverrides = @{} }
if ($null -eq $FixtureMetadata) { $FixtureMetadata = @{} }
if ($null -eq $ComparisonSelection) { $ComparisonSelection = @{} }
if ($null -eq $ControlledChangePaths) { $ControlledChangePaths = @() }
if ($WorkKind -eq 'edited-source' -and ([string]::IsNullOrWhiteSpace($ControlledChangeId) -or $ControlledChangePaths.Count -eq 0)) {
    throw 'Edited-source measurements require a controlled change ID and at least one explicit changed path.'
}

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
$environmentWasPresent = [ordered]@{}
foreach ($name in $environmentNames) {
    if ([string]::IsNullOrWhiteSpace($name) -or $name.Contains('=') -or $name.Contains([char]0)) {
        throw "Invalid environment variable name: $name"
    }
    $environmentWasPresent[$name] = Test-ProcessEnvironmentVariablePresent -Name $name
    $environmentBefore[$name] = if ($environmentWasPresent[$name]) {
        [Environment]::GetEnvironmentVariable($name, 'Process')
    }
    else {
        $null
    }
}

$ownedRun = $null
$comparisonContext = $null
$locationPushed = $false
$processStarted = $false
$executionCompleted = $false
$nativeExitCode = $null
$runnerError = $null
$restorationErrors = [System.Collections.Generic.List[string]]::new()
$recordingErrors = [System.Collections.Generic.List[string]]::new()
$leaseDisposalErrors = [System.Collections.Generic.List[string]]::new()
$auxiliaryPreflight = [pscustomobject][ordered]@{
    Policy = $AuxiliaryProcessPolicy
    Status = if ($AuxiliaryProcessPolicy -eq 'none') { 'NOT_REQUESTED' } else { 'NOT_STARTED' }
    Command = $null
    TimeoutSeconds = if ($AuxiliaryProcessPolicy -eq 'none') { $null } else { 20 }
    ElapsedSeconds = $null
    Candidates = @()
    Descriptors = @()
    Measurement = $null
    Error = $null
}
$auxiliaryImages = @()
$auxiliaryLeases = @()
$watch = $null
$startedUtc = $null
$endedUtc = $null

try {
    Push-Location -LiteralPath $workingPath
    $locationPushed = $true
    foreach ($name in $environmentNames) {
        $value = $EnvironmentOverrides[$name]
        if ($null -eq $value) {
            Remove-ProcessEnvironmentVariable -Name $name
        }
        else {
            [Environment]::SetEnvironmentVariable($name, [string]$value, 'Process')
        }
    }

    try {
        $isCargoCompile = $Stage -eq 'compile' -and [IO.Path]::GetFileName($resolvedExecutable) -ieq 'cargo.exe'
        $cargoPath = if ($isCargoCompile) { $resolvedExecutable } else { $null }
        $rustcPath = if ($isCargoCompile) { Resolve-RustcForComparison } else { $null }
        $comparisonContext = New-TrackBComparisonContext -SourceRoot $gitRootPath -WorkingDirectory $workingPath `
            -SourceSha $sourceSha -BaselineSha $BaselineSha -GitPath $gitPath -InvokedExecutablePath $resolvedExecutable -CargoPath $cargoPath -RustcPath $rustcPath `
            -ArgumentList @($ArgumentList) -TargetDirectory $effectiveTargetDirectory -TargetDirectorySource $targetDirectorySource `
            -Profile $Profile -WorkKind $WorkKind -CacheState $CacheState -ControlledChangeId $ControlledChangeId `
            -ControlledChangePaths @($ControlledChangePaths) -FingerprintFlags @($FingerprintFlags) -TargetFlags @($TargetFlags) `
            -Selection $ComparisonSelection -EnvironmentOverrides $EnvironmentOverrides `
            -ProcessProbe { param($path, $arguments, $timeout) Invoke-TextProcess -ExecutablePath $path -Arguments @($arguments) `
                -WorkingDirectory $workingPath -TimeoutSeconds $timeout }
    }
    catch {
        $comparisonContext = [pscustomobject][ordered]@{
            SchemaVersion = 1
            ProvenanceStatus = 'incomplete'
            CompatibilityCompleteness = $false
            CompatibilityFingerprintSha256 = $null
            Source = [pscustomobject][ordered]@{ Commit = $sourceSha; ChangedFiles = @(); Status = 'incomplete'; Sha256 = $null }
            Scenario = [pscustomobject][ordered]@{ WorkKind = $WorkKind; CacheState = $CacheState; ControlledChangeId = $ControlledChangeId; ControlledChangePaths = @($ControlledChangePaths) }
            Unknowns = @(('Comparison provenance collection failed: {0}' -f $_.Exception.Message))
        }
    }

    if ($AuxiliaryProcessPolicy -eq 'verified-msvc-vctip') {
        if (!(Test-VerifiedMsvcCargoCommand -Stage $Stage -Executable $resolvedExecutable -ArgumentList @($ArgumentList))) {
            throw 'MSVC helper preflight is restricted to explicit Cargo build/check/clippy or Nextest list preparation commands.'
        }
        $preflightResult = Invoke-VerifiedMsvcVctipPreflight -RunDirectory $runDirectory
        $auxiliaryPreflight = $preflightResult.Receipt
        $auxiliaryImages = @($preflightResult.Images)
        $auxiliaryLeases = @($preflightResult.Leases)
        if ($auxiliaryPreflight.Status -ne 'PASS') {
            foreach ($errorText in @($auxiliaryPreflight.LeaseCleanupErrors)) { $leaseDisposalErrors.Add([string]$errorText) }
            throw "MSVC helper trust preflight failed: $($auxiliaryPreflight.Error)"
        }
    }

    $startedUtc = [DateTimeOffset]::UtcNow
    $watch = [Diagnostics.Stopwatch]::StartNew()
    $ownedRun = Invoke-CheckedOwnedProcess -Executable $resolvedExecutable -ArgumentList @($ArgumentList) `
        -WorkingDirectory $workingPath -Stage $Stage -TimeoutSeconds $TimeoutSeconds `
        -CleanupTimeoutSeconds $CleanupTimeoutSeconds -AuxiliaryProcessPolicy $AuxiliaryProcessPolicy `
        -VerifiedMsvcVctipImages $auxiliaryImages -StdoutPath $stdoutPath -StderrPath $stderrPath
    $processStarted = $ownedRun.ProcessStarted
    $executionCompleted = $ownedRun.ProcessExited -and $ownedRun.CleanupVerified -and $ownedRun.OutputDrainVerified
    $nativeExitCode = $ownedRun.ExitCode
    foreach ($errorText in $ownedRun.RecordingErrors) { $recordingErrors.Add([string]$errorText) }
    foreach ($errorText in $ownedRun.CleanupErrors) { $recordingErrors.Add([string]$errorText) }
    if ($ownedRun.Reason -notin @('completed', 'failed exit')) { $runnerError = [string]$ownedRun.Reason }
    if ($AuxiliaryProcessPolicy -eq 'verified-msvc-vctip' -and $auxiliaryImages.Count -gt 0) {
        $postInvocationLeaseChecks = [System.Collections.Generic.List[object]]::new()
        foreach ($image in $auxiliaryImages) {
            try {
                $lease = $image.Lease
                if (!$lease.CanRead -or $lease.SafeFileHandle.IsClosed -or $lease.SafeFileHandle.IsInvalid) {
                    throw 'The held trusted-image file handle was closed before the measurement completed.'
                }
                $identity = [MultiLauncher.Dev.OwnedProcessNative]::GetOpenFileDescriptor($lease.SafeFileHandle)
                if ([string]$identity.FileId -cne [string]$image.Descriptor.FileId -or
                    [string]$identity.CanonicalPath -ine [string]$image.Descriptor.CanonicalPath) {
                    throw 'The held trusted-image file identity changed before lease release.'
                }
                $postInvocationLeaseChecks.Add([pscustomobject]@{
                    CanonicalPath = $identity.CanonicalPath
                    FileId = $identity.FileId
                    Verified = $true
                })
            }
            catch {
                $leaseDisposalErrors.Add(('Trusted MSVC image post-invocation verification: {0}' -f $_.Exception.Message))
            }
        }
        $auxiliaryPreflight | Add-Member -NotePropertyName PostInvocationLeaseChecks -NotePropertyValue @($postInvocationLeaseChecks) -Force
        if ($leaseDisposalErrors.Count -gt 0 -and !$runnerError) { $runnerError = 'Trusted MSVC image identity verification failed after invocation.' }
    }
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
        try {
            if ($environmentWasPresent[$name]) {
                [Environment]::SetEnvironmentVariable($name, [string]$environmentBefore[$name], 'Process')
                if (!(Test-ProcessEnvironmentVariablePresent -Name $name) -or
                    [Environment]::GetEnvironmentVariable($name, 'Process') -cne [string]$environmentBefore[$name]) {
                    throw 'The original environment value was not restored exactly.'
                }
            }
            else {
                Remove-ProcessEnvironmentVariable -Name $name
            }
        }
        catch { $restorationErrors.Add(('Environment restore {0}: {1}' -f $name, $_.Exception.Message)) }
    }
    if ($locationPushed) {
        try { Pop-Location }
        catch { $restorationErrors.Add(('Location restore: {0}' -f $_.Exception.Message)) }
    }
    foreach ($lease in $auxiliaryLeases) {
        try { $lease.Dispose() }
        catch { $leaseDisposalErrors.Add(('Trusted MSVC image lease disposal: {0}' -f $_.Exception.Message)) }
    }
    if ($leaseDisposalErrors.Count -gt 0 -and !$runnerError) { $runnerError = 'MSVC image lease cleanup was uncertain.' }
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
$finalCleanupErrors = [System.Collections.Generic.List[string]]::new()
if ($ownedRun) { foreach ($errorText in $ownedRun.CleanupErrors) { $finalCleanupErrors.Add([string]$errorText) } }
foreach ($errorText in $leaseDisposalErrors) { $finalCleanupErrors.Add([string]$errorText) }
$succeeded = $processStarted -and $executionCompleted -and $null -eq $runnerError -and $null -ne $ownedRun -and
    $ownedRun.Succeeded -and $nativeExitCode -eq 0 -and $restorationErrors.Count -eq 0 -and $recordingErrors.Count -eq 0 -and $leaseDisposalErrors.Count -eq 0
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
    CleanupErrors = @($finalCleanupErrors)
    OutputDrainVerified = if ($ownedRun) { $ownedRun.OutputDrainVerified } else { $false }
    OwnedProcesses = if ($ownedRun) { @($ownedRun.OwnedProcesses) } else { @() }
    PostCleanup = if ($ownedRun) { @($ownedRun.PostCleanup) } else { @() }
    OutputBytes = if ($ownedRun) { [pscustomobject]@{ StdoutRead = $ownedRun.StdoutBytesRead; StdoutRecorded = $ownedRun.StdoutBytesRecorded; StderrRead = $ownedRun.StderrBytesRead; StderrRecorded = $ownedRun.StderrBytesRecorded } } else { $null }
    RecordingErrors = @($recordingErrors)
    AuxiliaryProcessPolicy = $AuxiliaryProcessPolicy
    AuxiliaryPreflight = $auxiliaryPreflight
    AuxiliaryProcessClassifications = if ($ownedRun) { @($ownedRun.AuxiliaryProcessClassifications) } else { @() }
    ForcedJobRetirementAttempted = if ($ownedRun) { [bool]$ownedRun.ForcedJobRetirementAttempted } else { $false }
    AuxiliaryDisposition = if ($ownedRun) { [string]$ownedRun.AuxiliaryDisposition } elseif ($AuxiliaryProcessPolicy -eq 'none') { 'not-enabled' } else { [string]$auxiliaryPreflight.Status }
    AuxiliaryLeaseCleanupVerified = $leaseDisposalErrors.Count -eq 0
    AuxiliaryLeaseCleanupErrors = @($leaseDisposalErrors)
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
    WorkKind = $WorkKind
    ControlledChangeId = $ControlledChangeId
    ControlledChangePaths = @($ControlledChangePaths)
    ComparisonContext = $comparisonContext
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
