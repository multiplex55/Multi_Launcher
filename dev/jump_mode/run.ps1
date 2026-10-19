#requires -Version 7.0
[CmdletBinding()]
param(
    [switch]$FixturesOnly,
    [ValidateRange(5, 120)][int]$CaptureTimeoutSeconds = 45,
    [ValidateRange(1, 30)][int]$GracefulCloseSeconds = 6
)

$ErrorActionPreference = 'Stop'
$repoRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))
$cargoFeatures = 'dev-smoke-binaries,qa-framebuffer-capture'
$runStamp = [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssZ')
$runSuffix = [Guid]::NewGuid().ToString('N').Substring(0, 8)
$runId = "jump_m01_${runStamp}_$runSuffix"
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
$scenarioReason = 'Baseline capture has not started.'
$fixtureScenarioStatus = 'BLOCKED'
$fixtureScenarioReason = 'Fixture validation has not started.'
$cleanupIssues = [Collections.Generic.List[string]]::new()
$runStatus = 'BLOCKED'
$phase = 'initializing'
$failureText = $null
$interactive = [Environment]::UserInteractive
$sessionId = [Diagnostics.Process]::GetCurrentProcess().SessionId

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
        "# Jump Mode M01 baseline run $runId",
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
        $lines += "Baseline screenshot: ``$screenshotPath`` ($screenshotWidth x $screenshotHeight PNG pixels)"
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
    $baselineScenario = [ordered]@{
        scenario_id = 'm01-baseline-dashboard'
        status = $scenarioStatus
        expected_surface = 'actual Multi Launcher ROOT viewport with isolated note dashboard'
        capture_target = 'ROOT'
        capture_trigger = 'startup'
        screenshot = if ($screenshotPath) { 'screenshots/00_baseline_dashboard.png' } else { $null }
        pixel_width = $screenshotWidth
        pixel_height = $screenshotHeight
        reason = $scenarioReason
    }
    Write-Json $scenariosPath @($fixtureScenario, $baselineScenario)
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
        milestone = 'M01'
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
    Write-Summary 'BLOCKED' 'Runner is preparing a fresh profile and validating the M01 fixture before launch.'
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
    & cargo run --quiet --manifest-path (Join-Path $repoRoot 'Cargo.toml') --profile fast-dev --features $cargoFeatures --bin jump_mode_fixture -- generate $profileRoot 1> $fixtureStdoutPath 2> $fixtureStderrPath
    $fixtureGenerateExitCode = $LASTEXITCODE
    if ($fixtureGenerateExitCode -ne 0) { throw 'fixture generation failed; see fixture logs' }
    & cargo run --quiet --manifest-path (Join-Path $repoRoot 'Cargo.toml') --profile fast-dev --features $cargoFeatures --bin jump_mode_fixture -- verify $profileRoot 1>> $fixtureStdoutPath 2>> $fixtureStderrPath
    $fixtureVerifyExitCode = $LASTEXITCODE
    Write-Event 'fixture_generation_completed' @{ generate_exit_code = $fixtureGenerateExitCode; verify_exit_code = $fixtureVerifyExitCode }
    if ($fixtureVerifyExitCode -ne 0) { throw 'fixture verification failed; see fixture logs' }
    $fixtureManifestFile = Join-Path $profileRoot 'fixture_manifest.json'
    $fixtureHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $fixtureManifestFile).Hash.ToLowerInvariant()
    $fixtureScenarioStatus = 'PASS'
    $fixtureScenarioReason = 'Typed settings, dashboard, actions, plugin allowlist, isolated screenshot settings, and 32 stable note records passed fixture validation.'
    $phase = 'fixture_checked'
    Update-Scenarios
    Write-Summary 'BLOCKED' 'Fixture typed-load checks passed. The profile has an explicit `notes` plugin allowlist, empty custom actions, disabled user screenshot output, and 32 stable local note records.'

    if ($FixturesOnly) {
        $scenarioStatus = 'BLOCKED'
        $scenarioReason = 'Baseline app launch was not requested in fixture-only mode.'
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
        Write-Json $qaRequestPath ([ordered]@{
            run_id = $runId
            request_id = 'baseline-dashboard'
            target = 'ROOT'
            trigger = 'startup'
            capture_count = 1
            diagnostics = $false
            output_dir = $captureRoot
            fixture_manifest_sha256 = $fixtureHash
            build_receipt_identity = $buildReceiptIdentity
        })
        $psi = [Diagnostics.ProcessStartInfo]::new()
        $psi.FileName = $executablePath
        $psi.WorkingDirectory = $profileRoot
        $psi.UseShellExecute = $false
        $psi.CreateNoWindow = $false
        $psi.WindowStyle = [Diagnostics.ProcessWindowStyle]::Normal
        $psi.RedirectStandardOutput = $true
        $psi.RedirectStandardError = $true
        $psi.Environment['ML_NOTES_DIR'] = Join-Path $profileRoot 'notes'
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
            milestone = 'M01'
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
            capture = [ordered]@{ request = 'qa-framebuffer-request.json'; output_dir = 'capture'; source = 'qa-framebuffer-capture ROOT/Startup; CPU UI completion before GPU presentation' }
            cleanup = $null
        })
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
            milestone = 'M01'
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
                'Fixture validation passed. The baseline dashboard scenario remains blocked because this invocation did not request a GUI launch.'
            } else {
                'Fixture validation, ROOT capture, and owned-process cleanup passed.'
            }
            Write-Summary $runStatus $passDetails
        } elseif ($runStatus -eq 'NOT_RUN_ENVIRONMENT') {
            Write-Summary $runStatus 'Fixture verification passed. The GUI run was skipped because this process is outside an interactive Windows user session; no screenshot was fabricated.'
        } elseif ($runStatus -eq 'FAIL') {
            Write-Summary $runStatus 'The run did not meet its current acceptance criteria.'
        } elseif ($runStatus -eq 'BLOCKED') {
            Write-Summary $runStatus 'Fixture validation and real ROOT capture completed, with owned-process cleanup recorded. Dashboard fixture content remains blocked until a readiness signal can establish that isolated note results are visible.'
        }
        Write-Output "M01 status: $runStatus"
        Write-Output "Evidence: $evidenceRoot"
        if ($runStatus -eq 'FAIL') { exit 1 }
    }
    if ($null -eq $previousTargetDirectory) { Remove-Item Env:CARGO_TARGET_DIR -ErrorAction SilentlyContinue } else { $env:CARGO_TARGET_DIR = $previousTargetDirectory }
}
