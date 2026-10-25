#requires -Version 7.0
[CmdletBinding()]
param(
    [Parameter(Mandatory)][ValidateSet('SelectedBuild', 'SelectedRun', 'FullRun', 'Inventory')][string]$Mode,
    [Parameter(Mandatory)][ValidateSet('Library', 'Integration', 'Workspace')][string]$TargetKind,
    [string]$Target,
    [string]$Filter,
    [string]$CargoProfile,
    [ValidateSet('unknown', 'preparation', 'warm-noop', 'warm-edit')][string]$CacheState = 'unknown',
    [ValidateSet('libtest-json-plus', 'human')][string]$MessageFormat = 'libtest-json-plus',
    [ValidateRange(1, 900)][int]$TimeoutSeconds = 300,
    [string]$EvidenceDirectory = (Join-Path ([IO.Path]::GetTempPath()) 'Multi_Launcher\test-perf')
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'common.ps1')

$repoRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))
$manifestPath = Join-Path $repoRoot 'Cargo.toml'
if (!(Test-Path -LiteralPath $manifestPath -PathType Leaf)) { throw "Cargo.toml was not found at $manifestPath" }
if ($TargetKind -eq 'Integration' -and [string]::IsNullOrWhiteSpace($Target)) {
    throw 'Integration mode requires -Target <cargo-test-target>.'
}
if ($TargetKind -ne 'Integration' -and ![string]::IsNullOrWhiteSpace($Target)) {
    throw "$TargetKind mode does not accept -Target."
}
if ($Mode -in @('SelectedBuild', 'SelectedRun') -and $TargetKind -eq 'Workspace') {
    throw "$Mode requires an explicit Library or Integration target."
}
if ($Mode -eq 'FullRun' -and $TargetKind -ne 'Workspace') {
    throw 'FullRun must use -TargetKind Workspace to cover the unfiltered workspace.'
}
if ($Mode -eq 'FullRun' -and ![string]::IsNullOrWhiteSpace($Filter)) {
    throw 'FullRun is unfiltered; use SelectedRun with -Filter for a case selection.'
}
if ($CargoProfile -and $CargoProfile -notmatch '^[A-Za-z0-9_-]+$') { throw 'CargoProfile may contain only letters, digits, underscore and hyphen.' }

$evidenceRoot = [IO.Path]::GetFullPath($EvidenceDirectory)
if (Test-TestPerfPathWithin -Candidate $evidenceRoot -Directory $repoRoot) {
    throw "EvidenceDirectory must be outside the source worktree: $evidenceRoot"
}
[IO.Directory]::CreateDirectory($evidenceRoot) | Out-Null
$runName = '{0}_{1}_{2}' -f $Mode, [DateTimeOffset]::UtcNow.ToString('yyyyMMddTHHmmssfffZ'), [guid]::NewGuid().ToString('N')
$runDirectory = Join-Path $evidenceRoot $runName
[IO.Directory]::CreateDirectory($runDirectory) | Out-Null

function Get-CommandIdentity {
    param([Parameter(Mandatory)][string]$Name, [string[]]$Arguments)

    $application = Get-Command -Name $Name -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
    if (!$application) { return [pscustomobject]@{ Path = $null; Version = $null } }
    $path = if ($application.Source) { $application.Source } else { $application.Path }
    try {
        $result = Invoke-TestPerfCommandCapture -ExecutablePath $path -Arguments $Arguments -WorkingDirectory $repoRoot -TimeoutSeconds 15
        return [pscustomobject]@{ Path = $path; Version = if (!$result.TimedOut -and $result.ExitCode -eq 0) { $result.Stdout.Trim() } else { $null } }
    }
    catch { return [pscustomobject]@{ Path = $path; Version = $null } }
}

function Get-SourceIdentity {
    $git = Get-Command -Name git.exe -CommandType Application -ErrorAction Stop | Select-Object -First 1
    $gitPath = if ($git.Source) { $git.Source } else { $git.Path }
    $shaResult = Invoke-TestPerfCommandCapture -ExecutablePath $gitPath -Arguments @('-C', $repoRoot, 'rev-parse', 'HEAD') -WorkingDirectory $repoRoot
    if ($shaResult.ExitCode -ne 0) { throw "Could not read source SHA: $($shaResult.Stderr)" }
    $branchResult = Invoke-TestPerfCommandCapture -ExecutablePath $gitPath -Arguments @('-C', $repoRoot, 'branch', '--show-current') -WorkingDirectory $repoRoot
    $dirtyResult = Invoke-TestPerfCommandCapture -ExecutablePath $gitPath -Arguments @('-C', $repoRoot, 'status', '--short', '--untracked-files=all') -WorkingDirectory $repoRoot
    if ($dirtyResult.ExitCode -ne 0) { throw "Could not read Git status: $($dirtyResult.Stderr)" }
    $dirtyFiles = @($dirtyResult.Stdout -split '\r?\n' | Where-Object { $_.Length -gt 0 })
    $sourceFilesResult = Invoke-TestPerfCommandCapture -ExecutablePath $gitPath -Arguments @(
        '-C', $repoRoot, 'ls-files', '--cached', '--others', '--exclude-standard', '--',
        '*.rs', 'Cargo.toml', 'Cargo.lock', 'build.rs', '.config/nextest.toml', '.cargo/config.toml', '.cargo/config', 'dev/test_perf/*.ps1', 'dev/run_owned_process.ps1'
    ) -WorkingDirectory $repoRoot
    if ($sourceFilesResult.ExitCode -ne 0) { throw "Could not fingerprint source files: $($sourceFilesResult.Stderr)" }
    $fingerprintEntries = [System.Collections.Generic.List[string]]::new()
    foreach ($relativePath in ($sourceFilesResult.Stdout -split '\r?\n' | Where-Object { $_.Length -gt 0 } | Sort-Object -Unique)) {
        $absolutePath = Join-Path $repoRoot $relativePath
        if (!(Test-Path -LiteralPath $absolutePath -PathType Leaf)) { continue }
        $fileHash = (Get-FileHash -LiteralPath $absolutePath -Algorithm SHA256).Hash.ToLowerInvariant()
        $fingerprintEntries.Add("$relativePath`t$fileHash")
    }
    $fingerprintText = $fingerprintEntries -join "`n"
    $fingerprintBytes = [Text.UTF8Encoding]::new($false).GetBytes($fingerprintText)
    $fingerprintHash = [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($fingerprintBytes)).ToLowerInvariant()
    $dirtySourceFiles = @($dirtyFiles | ForEach-Object { if ($_.Length -gt 3) { $_.Substring(3) } } | Where-Object {
        $_ -match '(?i)(\.rs|\.toml|\.lock|\.ps1|(^|[\\/])build\.rs)$'
    })
    return [pscustomobject][ordered]@{
        Sha = $shaResult.Stdout.Trim()
        Branch = if ($branchResult.ExitCode -eq 0) { $branchResult.Stdout.Trim() } else { $null }
        Dirty = ($dirtyFiles.Count -gt 0)
        DirtyFiles = $dirtyFiles
        SourceDirty = ($dirtySourceFiles.Count -gt 0)
        DirtySourceFiles = $dirtySourceFiles
        SourceFingerprintSha256 = $fingerprintHash
    }
}

function New-TargetArguments {
    $targetArguments = [System.Collections.Generic.List[string]]::new()
    if ($TargetKind -eq 'Library') { $targetArguments.Add('--lib') }
    elseif ($TargetKind -eq 'Integration') { $targetArguments.Add('--test'); $targetArguments.Add($Target) }
    return @($targetArguments)
}

function Add-CommonArguments {
    param([System.Collections.Generic.List[string]]$Arguments)
    $Arguments.Add('--manifest-path')
    $Arguments.Add($manifestPath)
    if ($TargetKind -eq 'Workspace') { $Arguments.Add('--workspace') }
    else {
        $Arguments.Add('-p')
        $Arguments.Add('multi_launcher')
        foreach ($argument in (New-TargetArguments)) { $Arguments.Add($argument) }
    }
    if ($CargoProfile) { $Arguments.Add('--cargo-profile'); $Arguments.Add($CargoProfile) }
}

function Invoke-ProfilePhase {
    param(
        [Parameter(Mandatory)][string]$Name,
        [Parameter(Mandatory)][string[]]$Arguments,
        [ValidateSet('human', 'json', 'inventory')][string]$OutputMode = 'human',
        [hashtable]$EnvironmentOverrides = @{}
    )

    Write-Host "Phase: $Name"
    $slug = [regex]::Replace($Name, '[^A-Za-z0-9_-]+', '_').Trim('_')
    $stdoutPath = Join-Path $runDirectory ($slug + '.stdout.log')
    $stderrPath = Join-Path $runDirectory ($slug + '.stderr.log')
    $phase = Invoke-TestPerfProcess -ExecutablePath $cargo.Path -Arguments $Arguments -WorkingDirectory $repoRoot `
        -TimeoutSeconds $TimeoutSeconds -OwnedRunnerPath (Join-Path $repoRoot 'dev\run_owned_process.ps1') `
        -StdoutPath $stdoutPath -StderrPath $stderrPath -OutputMode $OutputMode `
        -EnvironmentOverrides $EnvironmentOverrides
    return [pscustomobject][ordered]@{
        Name = $Name
        Category = if ($Name -match '(?i)build|metadata') { 'Prebuild' } elseif ($Name -match '(?i)inventory') { 'Inventory' } elseif ($Name -match '(?i)execution') { 'Execution' } else { 'Other' }
        Command = 'cargo.exe'
        Arguments = @($Arguments)
        WallSeconds = $phase.WallSeconds
        ExitCode = $phase.ExitCode
        StartedUtc = $phase.StartedUtc
        EndedUtc = $phase.EndedUtc
        Status = $phase.Status
        TimedOut = $phase.TimedOut
        ProcessStarted = $phase.ProcessStarted
        ProcessCleanup = $phase.ProcessCleanup
        ProcessEvidenceDirectory = $phase.ProcessEvidenceDirectory
        OwnedProcessCount = $phase.OwnedProcessCount
        PostCleanupStates = @($phase.PostCleanupStates)
        RunnerError = $phase.RunnerError
        StdoutPath = $phase.StdoutPath
        StderrPath = $phase.StderrPath
        PerCaseResults = @($phase.PerCaseResults)
        ParserErrors = @($phase.ParserErrors)
    }
}

$phases = [System.Collections.Generic.List[object]]::new()
$testInventory = @()
$inventoryError = $null
$binariesMetadataPath = $null
$cargo = Get-Command -Name cargo.exe -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
$rustc = Get-Command -Name rustc.exe -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
$sourceIdentity = $null
$harnessError = $null
$exitCode = 0

try {
    $sourceIdentity = Get-SourceIdentity
    if (!$cargo) { throw 'cargo.exe was not found on PATH.' }
    if ($TargetKind -eq 'Library') { Write-Host 'Target: multi_launcher library (--lib)' }
    elseif ($TargetKind -eq 'Integration') { Write-Host "Target: multi_launcher integration test --test $Target" }
    else { Write-Host 'Target: entire workspace (--workspace, default features only)' }

    if ($Mode -eq 'SelectedBuild') {
        $arguments = [System.Collections.Generic.List[string]]::new()
        $arguments.Add('nextest'); $arguments.Add('list'); $arguments.Add('--list-type'); $arguments.Add('binaries-only'); $arguments.Add('--message-format'); $arguments.Add('json')
        Add-CommonArguments -Arguments $arguments
        $phase = Invoke-ProfilePhase -Name 'selected-build-and-binary-metadata' -Arguments @($arguments) -OutputMode inventory
        $phases.Add($phase)
        if ($phase.ExitCode -eq 0 -and !$phase.TimedOut) {
            $binariesMetadataPath = Join-Path $runDirectory 'binaries-metadata.json'
            Copy-Item -LiteralPath $phase.StdoutPath -Destination $binariesMetadataPath
        }
    }
    elseif ($Mode -eq 'Inventory') {
        $buildArguments = [System.Collections.Generic.List[string]]::new()
        $buildArguments.Add('nextest'); $buildArguments.Add('list'); $buildArguments.Add('--list-type'); $buildArguments.Add('binaries-only'); $buildArguments.Add('--message-format'); $buildArguments.Add('json')
        Add-CommonArguments -Arguments $buildArguments
        $buildPhase = Invoke-ProfilePhase -Name 'inventory-target-build-and-binary-metadata' -Arguments @($buildArguments) -OutputMode inventory
        $phases.Add($buildPhase)
        if ($buildPhase.ExitCode -eq 0 -and !$buildPhase.TimedOut) {
            $binariesMetadataPath = Join-Path $runDirectory 'binaries-metadata.json'
            Copy-Item -LiteralPath $buildPhase.StdoutPath -Destination $binariesMetadataPath
            $inventoryArguments = [System.Collections.Generic.List[string]]::new()
            $inventoryArguments.Add('nextest'); $inventoryArguments.Add('list'); $inventoryArguments.Add('--message-format'); $inventoryArguments.Add('json')
            $inventoryArguments.Add('--manifest-path'); $inventoryArguments.Add($manifestPath)
            $inventoryArguments.Add('--binaries-metadata'); $inventoryArguments.Add($binariesMetadataPath)
            if ($Filter) { $inventoryArguments.Add('-E'); $inventoryArguments.Add($Filter) }
            $phase = Invoke-ProfilePhase -Name 'test-inventory' -Arguments @($inventoryArguments) -OutputMode inventory
            $phases.Add($phase)
            if ($phase.ExitCode -eq 0 -and !$phase.TimedOut) {
                try { $testInventory = @(Convert-TestPerfInventory -Path $phase.StdoutPath) }
                catch { $inventoryError = $_.Exception.Message }
            }
        }
    }
    else {
        # Nextest's binary-only JSON list both builds the selected Cargo target and
        # supplies metadata consumed by the execution phase, keeping compilation
        # out of the measured run when this metadata is accepted by the installed CLI.
        $buildArguments = [System.Collections.Generic.List[string]]::new()
        $buildArguments.Add('nextest'); $buildArguments.Add('list'); $buildArguments.Add('--list-type'); $buildArguments.Add('binaries-only'); $buildArguments.Add('--message-format'); $buildArguments.Add('json')
        Add-CommonArguments -Arguments $buildArguments
        $buildPhase = Invoke-ProfilePhase -Name 'target-build-and-binary-metadata' -Arguments @($buildArguments) -OutputMode inventory
        $phases.Add($buildPhase)
        if ($buildPhase.ExitCode -ne 0 -or $buildPhase.TimedOut) { throw 'Target prebuild did not complete successfully.' }
        $binariesMetadataPath = Join-Path $runDirectory 'binaries-metadata.json'
        Copy-Item -LiteralPath $buildPhase.StdoutPath -Destination $binariesMetadataPath
        $binaryMetadata = Get-Content -LiteralPath $binariesMetadataPath -Raw -ErrorAction Stop | ConvertFrom-Json -AsHashtable -ErrorAction Stop
        if ($null -eq (Get-TestPerfField $binaryMetadata 'rust-binaries')) { throw 'Nextest binary metadata did not contain rust-binaries.' }

        $inventoryArguments = [System.Collections.Generic.List[string]]::new()
        $inventoryArguments.Add('nextest'); $inventoryArguments.Add('list'); $inventoryArguments.Add('--message-format'); $inventoryArguments.Add('json')
        $inventoryArguments.Add('--manifest-path'); $inventoryArguments.Add($manifestPath)
        $inventoryArguments.Add('--binaries-metadata'); $inventoryArguments.Add($binariesMetadataPath)
        if ($Filter) { $inventoryArguments.Add('-E'); $inventoryArguments.Add($Filter) }
        $inventoryPhase = Invoke-ProfilePhase -Name 'selected-test-inventory' -Arguments @($inventoryArguments) -OutputMode inventory
        $phases.Add($inventoryPhase)
        if ($inventoryPhase.ExitCode -eq 0 -and !$inventoryPhase.TimedOut) {
            try { $testInventory = @(Convert-TestPerfInventory -Path $inventoryPhase.StdoutPath) }
            catch { $inventoryError = $_.Exception.Message }
        }
        else { throw 'Selected test inventory did not complete successfully.' }
        if ($inventoryError) { throw "Invalid selected test inventory: $inventoryError" }

        $runArguments = [System.Collections.Generic.List[string]]::new()
        $runArguments.Add('nextest'); $runArguments.Add('run')
        $runArguments.Add('--manifest-path'); $runArguments.Add($manifestPath)
        $runArguments.Add('--binaries-metadata'); $runArguments.Add($binariesMetadataPath)
        if ($Filter) { $runArguments.Add('-E'); $runArguments.Add($Filter) }
        if ($Mode -eq 'FullRun') { $runArguments.Add('--no-fail-fast') }
        $format = if ($MessageFormat -eq 'libtest-json-plus') { 'libtest-json-plus' } else { 'human' }
        $runArguments.Add('--message-format'); $runArguments.Add($format)
        if ($format -eq 'libtest-json-plus') { $runArguments.Add('--message-format-version'); $runArguments.Add('0.1') }
        $environment = @{}
        $outputMode = if ($format -eq 'libtest-json-plus') { 'json' } else { 'human' }
        if ($format -eq 'libtest-json-plus') { $environment['NEXTEST_EXPERIMENTAL_LIBTEST_JSON'] = '1' }
        $runPhase = Invoke-ProfilePhase -Name 'test-execution' -Arguments @($runArguments) -OutputMode $outputMode -EnvironmentOverrides $environment
        $phases.Add($runPhase)
    }
}
catch {
    $harnessError = '{0}: {1}' -f $_.Exception.GetType().FullName, $_.Exception.Message
    [Console]::Error.WriteLine("Harness error: $harnessError")
}

$phaseFailure = $phases | Where-Object { $_.Status -ne 'PASS' } | Select-Object -First 1
if ($phaseFailure) {
    $exitCode = if ($phaseFailure.TimedOut) { 124 } elseif ($null -ne $phaseFailure.ExitCode -and $phaseFailure.ExitCode -ne 0) { [int]$phaseFailure.ExitCode } else { 1 }
}
if ($null -ne $harnessError) { $exitCode = 1 }
$parserErrors = @($phases | ForEach-Object { $_.ParserErrors } | Where-Object { $_ })
if ($inventoryError -or $parserErrors.Count -gt 0) {
    $harnessError = 'Structured evidence could not be parsed completely; inspect raw logs and parser errors.'
    if ($exitCode -eq 0) { $exitCode = 1 }
}
$inventoryCollected = !$inventoryError -and @($phases | Where-Object { $_.Name -in @('test-inventory', 'selected-test-inventory') -and $_.Status -eq 'PASS' }).Count -gt 0
$executionPhase = $phases | Where-Object Name -eq 'test-execution' | Select-Object -First 1
$resultsCollected = $null -ne $executionPhase -and $MessageFormat -eq 'libtest-json-plus' -and $parserErrors.Count -eq 0
$passed = @($executionPhase.PerCaseResults | Where-Object Status -eq 'PASS').Count
$failed = @($executionPhase.PerCaseResults | Where-Object Status -eq 'FAIL').Count
$timedOutCases = @($executionPhase.PerCaseResults | Where-Object Status -eq 'TIMEOUT').Count
$ignored = @($executionPhase.PerCaseResults | Where-Object Status -eq 'IGNORED').Count
$notIgnoredInventory = Get-TestPerfSelectedCaseCount -Cases $testInventory
$notIgnoredInventory = [int]$notIgnoredInventory
if ($Mode -eq 'Inventory' -and $notIgnoredInventory -eq 0 -and !$phaseFailure) { $exitCode = 4 }
$runNoMatches = $Mode -in @('SelectedRun', 'FullRun') -and $executionPhase -and $executionPhase.ExitCode -eq 0 -and $executionPhase.PerCaseResults.Count -eq 0 -and $notIgnoredInventory -eq 0 -and !$inventoryError
if ($runNoMatches -and $exitCode -eq 0) {
    $exitCode = 4
    Write-Warning 'Nextest reported no runnable cases for this target/filter; the command is not counted as a pass.'
}

$git = Get-Command -Name git.exe -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
$gitPath = if ($git) { if ($git.Source) { $git.Source } else { $git.Path } } else { $null }
$cargoPath = if ($cargo) { if ($cargo.Source) { $cargo.Source } else { $cargo.Path } } else { $null }
$rustcPath = if ($rustc) { if ($rustc.Source) { $rustc.Source } else { $rustc.Path } } else { $null }
$nextest = if ($cargoPath) { Get-CommandIdentity -Name 'cargo.exe' -Arguments @('nextest', '--version') } else { [pscustomobject]@{Path=$null;Version=$null} }
$cargoVersion = $null
$rustcVersion = $null
try { if ($cargoPath) { $cargoVersion = (Invoke-TestPerfCommandCapture -ExecutablePath $cargoPath -Arguments @('--version') -WorkingDirectory $repoRoot).Stdout.Trim() } } catch {}
try { if ($rustcPath) { $rustcVersion = (Invoke-TestPerfCommandCapture -ExecutablePath $rustcPath -Arguments @('--version') -WorkingDirectory $repoRoot).Stdout.Trim() } } catch {}
$receipt = [pscustomobject][ordered]@{
    SchemaVersion = 1
    Harness = 'multi-launcher-test-perf'
    Mode = $Mode
    Status = if ($exitCode -eq 0) { 'PASS' } elseif ($exitCode -eq 4) { 'NO_MATCHES' } elseif ($phases | Where-Object TimedOut) { 'TIMEOUT' } else { 'FAIL' }
    ExitCode = $exitCode
    StartedAtUtc = if ($phases.Count) { $phases[0].StartedUtc } else { $null }
    Target = [pscustomobject]@{
        Package = if ($TargetKind -eq 'Workspace') { $null } else { 'multi_launcher' }
        Kind = $TargetKind
        Name = if ($TargetKind -eq 'Library') { 'lib' } elseif ($TargetKind -eq 'Integration') { $Target } else { 'workspace' }
    }
    Filter = if ($Filter) { $Filter } else { $null }
    CargoProfile = if ($CargoProfile) { $CargoProfile } else { 'workspace default (not overridden)' }
    CacheState = $CacheState
    WallTimeDefinition = 'Contained phase wall time, including process inventory and cleanup overhead; not pure linker time.'
    Features = [pscustomobject]@{ DefaultFeatures = $true; Explicit = @() }
    MessageFormat = if ($Mode -in @('SelectedRun', 'FullRun')) { $MessageFormat } else { $null }
    MessageFormatVersion = if ($Mode -in @('SelectedRun', 'FullRun') -and $MessageFormat -eq 'libtest-json-plus') { '0.1' } else { $null }
    Source = $sourceIdentity
    Toolchain = [pscustomobject]@{
        Cargo = $cargoVersion
        Rustc = $rustcVersion
        Nextest = if ($nextest) { $nextest.Version } else { $null }
        PowerShell = $PSVersionTable.PSVersion.ToString()
        OS = [Runtime.InteropServices.RuntimeInformation]::OSDescription
        Architecture = [Runtime.InteropServices.RuntimeInformation]::ProcessArchitecture.ToString()
        LogicalProcessors = [Environment]::ProcessorCount
        CPU = $(try { (Get-ItemProperty 'HKLM:\HARDWARE\DESCRIPTION\System\CentralProcessor\0' -ErrorAction Stop).ProcessorNameString } catch { $null })
    }
    Environment = [pscustomobject]@{
        CargoBuildJobs = $env:CARGO_BUILD_JOBS
        CargoTargetDirectory = $env:CARGO_TARGET_DIR
        NextestProfile = if ($env:NEXTEST_PROFILE) { $env:NEXTEST_PROFILE } else { 'default' }
        NextestTestThreads = $env:NEXTEST_TEST_THREADS
        ExperimentalJsonEnabledForChild = ($Mode -in @('SelectedRun', 'FullRun') -and $MessageFormat -eq 'libtest-json-plus')
    }
    TimeoutSeconds = $TimeoutSeconds
    ProcessCleanup = @($phases | ForEach-Object { [pscustomobject]@{ Phase = $_.Name; Status = $_.ProcessCleanup } })
    BinariesMetadataPath = $binariesMetadataPath
    BinariesMetadataSha256 = if ($binariesMetadataPath -and (Test-Path -LiteralPath $binariesMetadataPath)) { (Get-FileHash -LiteralPath $binariesMetadataPath -Algorithm SHA256).Hash.ToLowerInvariant() } else { $null }
    Inventory = [pscustomobject]@{
        Source = if ($testInventory.Count -gt 0) { 'nextest list JSON' } elseif ($inventoryError) { 'unavailable' } else { 'not collected or empty' }
        TotalCases = if ($inventoryCollected) { $testInventory.Count } else { $null }
        IgnoredCases = if ($inventoryCollected) { @($testInventory | Where-Object Ignored).Count } else { $null }
        SelectedCases = if ($inventoryCollected) { $notIgnoredInventory } else { $null }
        Cases = @($testInventory)
        ParserError = $inventoryError
    }
    Results = [pscustomobject]@{
        Passed = if ($resultsCollected) { $passed } else { $null }
        Failed = if ($resultsCollected) { $failed } else { $null }
        TimedOut = if ($resultsCollected) { $timedOutCases } else { $null }
        Ignored = if ($inventoryCollected) { @($testInventory | Where-Object Ignored).Count } else { $null }
        PerCase = @($executionPhase.PerCaseResults)
    }
    Phases = @($phases)
    HarnessError = $harnessError
    EvidenceDirectory = $runDirectory
}
$receiptPath = Join-Path $runDirectory 'receipt.json'
$receipt | ConvertTo-Json -Depth 14 | Set-Content -LiteralPath $receiptPath -Encoding utf8
Write-Host "Receipt: $receiptPath"
Write-Host ("Status: {0}; build phases: {1}; passed: {2}; failed: {3}; timeout: {4}; ignored: {5}" -f $receipt.Status, $phases.Count, $passed, $failed, $timedOutCases, $ignored)
exit $exitCode
