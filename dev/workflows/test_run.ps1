#requires -Version 7.0
[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'engine.ps1')

function Assert-WorkflowEqual {
    param([object]$Actual, [object]$Expected, [string]$Label)
    if ($Actual -cne $Expected) { throw "$Label expected '$Expected', got '$Actual'." }
}

function Assert-WorkflowSequence {
    param([object[]]$Actual, [object[]]$Expected, [string]$Label)
    if ($Actual.Count -ne $Expected.Count) { throw "$Label expected $($Expected.Count) arguments, got $($Actual.Count): $($Actual -join '|')" }
    for ($index = 0; $index -lt $Expected.Count; $index++) {
        if ([string]$Actual[$index] -cne [string]$Expected[$index]) {
            throw "$Label argument $index expected '$($Expected[$index])', got '$($Actual[$index])'."
        }
    }
}

function Assert-WorkflowThrows {
    param([scriptblock]$Action, [string]$Label)
    $threw = $false
    try { & $Action | Out-Null } catch { $threw = $true }
    if (!$threw) { throw "$Label was expected to fail closed." }
}

$measurementFactory = {
    param(
        $Spec,
        [string]$Stdout = '',
        [int]$ExitCode = 0,
        [bool]$Succeeded = $true,
        [bool]$TimedOut = $false,
        [bool]$CleanupVerified = $true,
        [bool]$OutputDrainVerified = $true
    )
    $phaseDirectory = Join-Path $Spec.EvidenceDirectory ('fixture-' + [guid]::NewGuid().ToString('N'))
    [IO.Directory]::CreateDirectory($phaseDirectory) | Out-Null
    $stdoutPath = Join-Path $phaseDirectory 'stdout.json'
    $stderrPath = Join-Path $phaseDirectory 'stderr.log'
    $manifestPath = Join-Path $phaseDirectory 'measurement.json'
    Set-Content -LiteralPath $stdoutPath -Value $Stdout -Encoding utf8
    Set-Content -LiteralPath $stderrPath -Value '' -Encoding utf8
    [pscustomobject][ordered]@{
        DeadlineUtc = [DateTimeOffset]::UtcNow.AddSeconds([int]$Spec.TimeoutSeconds).ToString('o')
        TimeoutSeconds = [int]$Spec.TimeoutSeconds
        TimedOut = $TimedOut
        ExitCode = if ($TimedOut) { $null } else { $ExitCode }
        Succeeded = $Succeeded -and !$TimedOut -and $CleanupVerified -and $OutputDrainVerified -and $ExitCode -eq 0
        ExecutionReason = if ($TimedOut) { 'deadline expired' } elseif ($Succeeded) { 'completed' } else { 'fixture failure' }
        ProcessStarted = $true
        CleanupVerified = $CleanupVerified
        CleanupErrors = if ($CleanupVerified) { @() } else { @('fixture cleanup was not verified') }
        OutputDrainVerified = $OutputDrainVerified
        OutputBytes = [pscustomobject]@{ StdoutRead = $Stdout.Length; StdoutRecorded = $Stdout.Length; StderrRead = 0; StderrRecorded = 0 }
        OwnedProcesses = @([pscustomobject]@{ ProcessId = 424242; CreationTimeUtc = [DateTimeOffset]::UtcNow.ToString('o') })
        PostCleanup = @([pscustomobject]@{ ProcessId = 424242; State = if ($CleanupVerified) { 'retired' } else { 'unknown' } })
        RecordingErrors = @()
        EvidenceDirectory = $phaseDirectory
        ManifestPath = $manifestPath
        StdoutPath = $stdoutPath
        StderrPath = $stderrPath
    }
}.GetNewClosure()

$fixtureRunId = '4b647885-2e45-4e11-987c-63d86da13140'
# These deterministic JSON events are synthetic contract fixtures for the reducer and facade.
# Native Nextest capture remains unverified; a fixture does not stand in for that proof.
function New-WorkflowFixtureRunOutput {
    param([Parameter(Mandatory)][string]$TestStatus)
    $selection = @(Get-WorkflowCatalog | ForEach-Object { $_.selections | Where-Object name -ceq 'tables' })[0]
    $name = '{0}::{1}${2}' -f $selection.package.Replace('-', '_'), $selection.targetName.Replace('-', '_'), $selection.exactCase
    $event = if ($TestStatus -eq 'FAIL') { 'failed' } else { 'ok' }
    $suiteEvent = if ($TestStatus -eq 'FAIL') { 'failed' } else { 'ok' }
    $suite = [ordered]@{ type = 'suite'; event = 'started'; nextest = @{ run_id = $fixtureRunId } }
    $started = [ordered]@{ type = 'test'; event = 'started'; name = $name; nextest = @{ status = 'running' } }
    $terminal = [ordered]@{ type = 'test'; event = $event; name = $name; exec_time = 0.012; nextest = @{ status = $TestStatus.ToLowerInvariant(); time = @{ exec_time = 0.012 } } }
    $suiteTerminal = [ordered]@{ type = 'suite'; event = $suiteEvent; nextest = @{ run_id = $fixtureRunId } }
    return (@($suite, $started, $terminal, $suiteTerminal) | ForEach-Object { ConvertTo-Json -InputObject $_ -Compress -Depth 5 }) -join "`n"
}
$fixturePassRunOutput = New-WorkflowFixtureRunOutput -TestStatus PASS
$fixtureFailRunOutput = New-WorkflowFixtureRunOutput -TestStatus FAIL

$catalog = Get-WorkflowCatalog
$expectedCases = @(
    'tables|lib|table_utility::tests::table_parse_csv_quotes_escaped_quotes_and_embedded_newlines_preserve_strings|src/table_utility.rs',
    'notes|lib|plugins::note::tests::extract_entity_refs_parses_link_note_urls_like_action_links|src/plugins/note.rs',
    'paste|lib|paste_sequence::tests::clipboard_reference_uses_exact_utf8_sha256_and_redacted_debug|src/paste_sequence.rs',
    'query-focus|lib|gui::tests::launcher_query_is_not_refocused_while_file_search_remains_open|src/gui/mod.rs',
    'settings|lib|settings::model::tests::query_results_layout_defaults_are_backward_compatible|src/settings/model.rs',
    'integration-query|integration|query_autocomplete::default_query_autocomplete_enabled|tests/pure_cases/query_autocomplete.rs'
)
Assert-WorkflowEqual $catalog.selections.Count 6 'Catalog selection count'
foreach ($entry in $expectedCases) {
    $parts = $entry.Split('|')
    $selection = @($catalog.selections | Where-Object { $_.name -ceq $parts[0] })
    Assert-WorkflowEqual $selection.Count 1 "Catalog entry $($parts[0])"
    Assert-WorkflowEqual $selection[0].targetKind $parts[1] "$($parts[0]) target kind"
    Assert-WorkflowEqual $selection[0].exactCase $parts[2] "$($parts[0]) exact case"
    Assert-WorkflowEqual $selection[0].source $parts[3] "$($parts[0]) source"
    Assert-WorkflowEqual $selection[0].features.defaultFeatures $true "$($parts[0]) default features"
    Assert-WorkflowEqual $selection[0].features.explicit.Count 0 "$($parts[0]) explicit feature set"
    $functionName = ($parts[2] -split '::')[-1]
    $sourcePath = Join-Path $script:WorkflowRoot $parts[3]
    $sourcePattern = '(?m)^\s*(?:async\s+)?fn\s+' + [regex]::Escape($functionName) + '\s*\('
    if (!(Select-String -LiteralPath $sourcePath -Pattern $sourcePattern -Quiet -ErrorAction Stop)) {
        throw "Catalog selection '$($parts[0])' is not backed by source function '$functionName' in '$($parts[3])'."
    }
}

$manifestPath = [IO.Path]::GetFullPath((Join-Path $script:WorkflowRoot 'Cargo.toml'))
$modeCases = @(
    @{ Mode = 'quick-check'; Expected = @('check', '--lib', '--manifest-path', $manifestPath) },
    @{ Mode = 'full-debug'; Expected = @('build', '--bin', 'multi_launcher', '--manifest-path', $manifestPath) },
    @{ Mode = 'fast-dev-build'; Expected = @('build', '--profile', 'fast-dev', '--bin', 'multi_launcher', '--manifest-path', $manifestPath) },
    @{ Mode = 'iteration'; Expected = @('build', '--profile', 'iteration', '--bin', 'multi_launcher', '--manifest-path', $manifestPath) },
    @{ Mode = 'production'; Expected = @('build', '--release', '--bin', 'multi_launcher', '--manifest-path', $manifestPath) }
)
foreach ($case in $modeCases) {
    $request = New-WorkflowRequest -Action Preview -Mode $case.Mode
    Assert-WorkflowEqual $request.Status 'RESOLVED' "$($case.Mode) resolution"
    Assert-WorkflowSequence @($request.Phases[0].Command.ArgumentList) @($case.Expected) "$($case.Mode) command"
    Assert-WorkflowEqual $request.Features.DefaultFeatures $true "$($case.Mode) default features"
    Assert-WorkflowEqual $request.Features.Explicit.Count 0 "$($case.Mode) explicit features"
}

$quickCheckRequest = New-WorkflowRequest -Action Preview -Mode quick-check
Assert-WorkflowEqual $quickCheckRequest.TargetKind 'lib' 'Quick-check target kind metadata'
Assert-WorkflowEqual $quickCheckRequest.TargetName 'multi_launcher' 'Quick-check target name metadata'

$legacyCases = @(
    @{ Preset = 'release'; Arguments = @('build', '--release', '--bin', 'multi_launcher', '--manifest-path', $manifestPath) },
    @{ Preset = 'debug'; Arguments = @('build', '--bin', 'multi_launcher', '--manifest-path', $manifestPath) },
    @{ Preset = 'lib-test'; Arguments = @('nextest', 'run', '--lib', '--manifest-path', $manifestPath) },
    @{ Preset = 'integration-test'; TestTarget = 'pure'; Arguments = @('nextest', 'run', '--test', 'pure', '--manifest-path', $manifestPath) }
)
foreach ($legacy in $legacyCases) {
    $arguments = @{ Preset = $legacy.Preset; PrintCommand = $true }
    if ($legacy.TestTarget) { $arguments.TestTarget = $legacy.TestTarget }
    $command = (& (Join-Path $script:WorkflowRoot 'dev\track_b\cargo.ps1') @arguments) | ConvertFrom-Json
    Assert-WorkflowEqual $command.Started $false "$($legacy.Preset) preview state"
    Assert-WorkflowSequence @($command.ArgumentList) @($legacy.Arguments) "$($legacy.Preset) compatibility"
}

$cliRunRequest = New-WorkflowRequestFromCli -Action Run -Mode fast-dev-tests -SelectionName tables
Assert-WorkflowEqual $cliRunRequest.Status 'RESOLVED' 'CLI run without optional target kind'
Assert-WorkflowEqual $cliRunRequest.TargetKind 'lib' 'CLI selection resolves target kind'

$fixtureSmokeRequest = New-WorkflowRequestFromCli -Action Preview -SmokeSelectionName phase2-settings-fixture
Assert-WorkflowEqual $fixtureSmokeRequest.WorkflowKind 'jump-smoke' 'Settings fixture workflow kind'
Assert-WorkflowEqual $fixtureSmokeRequest.CaptureCount 0 'Settings fixture selection capture count'
Assert-WorkflowSequence @($fixtureSmokeRequest.Phases[0].Command.ArgumentList) @(
    '-NoLogo', '-NoProfile', '-NonInteractive', '-File', [IO.Path]::GetFullPath($script:WorkflowJumpHarnessPath),
    '-FixturesOnly', '-Scenario', 'Phase2', '-Surface', 'Settings', '-CaptureTimeoutSeconds', '45',
    '-GracefulCloseSeconds', '6', '-OverallTimeoutSeconds', '3600', '-WorkflowReceiptPath',
    (Join-Path ([IO.Path]::GetTempPath()) 'MultiLauncher\workflow-evidence-preview\jump-run.json')
) 'Settings fixture command'
$nativeSmokeRequest = New-WorkflowRequestFromCli -Action Preview -SmokeSelectionName phase2-settings-native
Assert-WorkflowEqual $nativeSmokeRequest.CaptureCount 3 'Settings native selection capture count'
Assert-WorkflowEqual $nativeSmokeRequest.BudgetSeconds.postBuildNativeSeconds 300 'Settings native hard deadline'
Assert-WorkflowSequence @($nativeSmokeRequest.NativePrerequisites) @(
    'Windows interactive user session', 'owned foreground window', 'native input', 'framebuffer capture', 'owned process inventory and cleanup'
) 'Settings native prerequisites'
if (@($nativeSmokeRequest.Phases[0].Command.ArgumentList) -contains '-ExecutionPolicy' -or
    @($nativeSmokeRequest.Phases[0].Command.ArgumentList) -contains 'Bypass') {
    throw 'Settings smoke command added an unsupported PowerShell execution-policy bypass.'
}
$smokePreviewChildCalls = 0
$smokePreviewResult = Invoke-WorkflowAction -Request $fixtureSmokeRequest -ProcessInvoker { $smokePreviewChildCalls++; throw 'Preview unexpectedly launched a child.' }
Assert-WorkflowEqual $smokePreviewResult.Status 'PREVIEW' 'Settings workflow preview'
Assert-WorkflowEqual $smokePreviewChildCalls 0 'Settings workflow preview has no child process'
Assert-WorkflowThrows {
    New-WorkflowRequestFromCli -Action Run -Mode fast-dev-tests -SmokeSelectionName phase2-settings-native
} 'Mixed workflow and smoke arguments rejection'
$tamperedSmokeSelection = ConvertFrom-Json -InputObject (ConvertTo-Json -InputObject (Get-WorkflowSmokeSelection -SmokeSelectionName phase2-settings-native) -Depth 10) -AsHashtable
$tamperedSmokeSelection.harnessArguments[1] = 'Phase1'
Assert-WorkflowThrows {
    New-WorkflowSmokeCommand -Selection $tamperedSmokeSelection -ReceiptPath (Join-Path ([IO.Path]::GetTempPath()) 'jump-run.json')
} 'Tampered Settings harness argument rejection before child launch'

$cargoWrapper = Join-Path $script:WorkflowRoot 'dev\track_b\cargo.ps1'
Assert-WorkflowThrows { & $cargoWrapper -Preset quick-check -CargoArguments @('--all-targets') -PrintCommand } 'Long selector conflict'
Assert-WorkflowThrows { & $cargoWrapper -Preset full-debug -CargoArguments @('--package=other') -PrintCommand } 'Equals selector conflict'
Assert-WorkflowThrows { & $cargoWrapper -Preset full-debug -CargoArguments @('-pother') -PrintCommand } 'Short selector conflict'

$exactCase = 'table_utility::tests::table_parse_csv_quotes_escaped_quotes_and_embedded_newlines_preserve_strings'
$runArguments = @('--exact', $exactCase)
$runRequest = New-WorkflowRequest -Action Run -Mode fast-dev-tests -SelectionName tables -RunnerProfile 'ci-fast' -TestArguments $runArguments
$opaqueArguments = @('quoted "argument" with spaces', 'filter!:punctuation')
$opaqueArgumentsRequest = New-WorkflowRequest -Action Run -Mode fast-dev-tests -SelectionName tables -TestArguments $opaqueArguments
$toolReceipt = New-WorkflowToolReceipt -Tool $runRequest.NextestTool
$toolReceiptRoundTrip = ConvertFrom-Json -InputObject (ConvertTo-Json -InputObject $toolReceipt -Depth 4) -AsHashtable
Assert-WorkflowEqual (@($toolReceiptRoundTrip.Keys) -join '|') 'path|sha256' 'Lowercase tool receipt schema'
Assert-WorkflowEqual $runRequest.CompilerProfile 'fast-dev' 'Compiler profile stays distinct'
Assert-WorkflowEqual $runRequest.RunnerProfile 'ci-fast' 'Nextest runner profile'
Assert-WorkflowEqual $runRequest.PrepareCommand.ArgumentList[7] 'fast-dev' 'Cargo Nextest compiler profile flag'
Assert-WorkflowEqual $runRequest.RunCommand.ArgumentList[$runRequest.RunCommand.ArgumentList.Count - $runArguments.Count - 1] '--' 'Test-binary separator'
$forwardedStart = $runRequest.RunCommand.ArgumentList.Count - $runArguments.Count
$forwardedArguments = @($runRequest.RunCommand.ArgumentList[$forwardedStart..($runRequest.RunCommand.ArgumentList.Count - 1)])
Assert-WorkflowSequence $forwardedArguments $runArguments 'Forwarded test arguments'
$opaqueStart = $opaqueArgumentsRequest.RunCommand.ArgumentList.Count - $opaqueArguments.Count
Assert-WorkflowSequence @($opaqueArgumentsRequest.RunCommand.ArgumentList[$opaqueStart..($opaqueArgumentsRequest.RunCommand.ArgumentList.Count - 1)]) $opaqueArguments 'Opaque spaced and punctuated test arguments'
Assert-WorkflowEqual $runRequest.RunCommand.ArgumentList[[Array]::IndexOf([string[]]$runRequest.RunCommand.ArgumentList, '-P') + 1] 'ci-fast' 'Nextest profile argument'
Assert-WorkflowEqual $runRequest.RunCommand.ArgumentList[[Array]::IndexOf([string[]]$runRequest.RunCommand.ArgumentList, '--message-format') + 1] 'libtest-json-plus' 'Structured result format'
Assert-WorkflowEqual $runRequest.RunCommand.ArgumentList[[Array]::IndexOf([string[]]$runRequest.RunCommand.ArgumentList, '--message-format-version') + 1] '0.1' 'Structured result version'
$punctuationFilter = Get-WorkflowFilterExpression -ExactCase 'case::with/slash "quote" and punctuation!?'
if (!$punctuationFilter.Contains('\/') -or !$punctuationFilter.Contains('\?') -or !$punctuationFilter.Contains('"quote"')) {
    throw "Exact filter did not safely preserve regex delimiters and punctuation: $punctuationFilter"
}
Assert-WorkflowThrows { New-WorkflowRequest -Action Run -Mode fast-dev-tests -SelectionName unknown } 'Unknown catalog selection'
Assert-WorkflowThrows { New-WorkflowRequest -Action Run -Mode fast-dev-tests -SelectionName tables -TargetKind integration } 'Conflicting selection target'
$captureGuardCalls = 0
foreach ($unsupportedCaptureArgument in @('--nocapture', '--no-capture')) {
    $captureRejected = $false
    try {
        $captureRequest = New-WorkflowRequest -Action Run -Mode fast-dev-tests -SelectionName tables -TestArguments @($unsupportedCaptureArgument)
        Invoke-WorkflowAction -Request $captureRequest -ProcessInvoker { $captureGuardCalls++; throw 'Capture guard unexpectedly launched a child.' }
    }
    catch { $captureRejected = $_.Exception.Message.Contains('require captured test output') }
    Assert-WorkflowEqual $captureRejected $true "Structured capture guard for $unsupportedCaptureArgument"
}
Assert-WorkflowEqual $captureGuardCalls 0 'Capture-disabling requests fail before child launch'

$savedPath = $env:PATH
$emptyPath = Join-Path ([IO.Path]::GetTempPath()) ('workflow-empty-path-' + [guid]::NewGuid().ToString('N'))
[IO.Directory]::CreateDirectory($emptyPath) | Out-Null
try {
    $env:PATH = $emptyPath
    Assert-WorkflowThrows { Get-WorkflowNextestIdentity } 'Missing Cargo Nextest executable'
}
finally {
    $env:PATH = $savedPath
    [IO.Directory]::Delete($emptyPath, $true)
}

$testRoot = Join-Path ([IO.Path]::GetTempPath()) ('Multi Launcher\workflow self test-' + [guid]::NewGuid().ToString('N'))
[IO.Directory]::CreateDirectory($testRoot) | Out-Null
$syntheticJumpEvidenceRoots = [System.Collections.Generic.List[string]]::new()
try {
    $makeSyntheticJumpRun = {
        param($Spec, [string]$SelectionName, [string]$Outcome)
        $receiptIndex = [Array]::IndexOf([string[]]$Spec.Command.ArgumentList, '-WorkflowReceiptPath')
        if ($receiptIndex -lt 0 -or $receiptIndex + 1 -ge $Spec.Command.ArgumentList.Count) { throw 'Smoke command did not pass its generated receipt path.' }
        $linkPath = [IO.Path]::GetFullPath([string]$Spec.Command.ArgumentList[$receiptIndex + 1])
        $runId = 'jump_m13_phase2_settings_{0}_{1}' -f [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssZ'), [guid]::NewGuid().ToString('N').Substring(0, 8)
        $harnessRoot = Join-Path $script:WorkflowRoot "dev\jump_mode\evidence\$runId"
        [IO.Directory]::CreateDirectory($harnessRoot) | Out-Null
        $syntheticJumpEvidenceRoots.Add($harnessRoot)
        $measureRoot = Join-Path ([IO.Path]::GetDirectoryName($linkPath)) 'jump-measurements'
        $cargo = Get-WorkflowCargoPath
        $manifest = Join-Path $script:WorkflowRoot 'Cargo.toml'
        $sourceSha = 'd' * 40
        $fixtureProfile = Join-Path $harnessRoot 'profile'
        $definitions = @(
            @{ Name = 'cargo-metadata'; Stage = 'provenance'; Executable = $cargo; Arguments = @('metadata', '--no-deps', '--format-version', '1', '--manifest-path', $manifest); Policy = 'none' },
            @{ Name = 'fixture-binary-prepare'; Stage = 'compile'; Executable = $cargo; Arguments = @('nextest', 'list', '--list-type', 'binaries-only', '--message-format', 'json', '--cargo-profile', 'fast-dev', '--features', 'dev-smoke-binaries,qa-framebuffer-capture', '--bin', 'jump_mode_fixture', '--manifest-path', $manifest); Policy = 'verified-msvc-vctip' },
            @{ Name = 'fixture-generator-build'; Stage = 'compile'; Executable = $cargo; Arguments = @('build', '--manifest-path', $manifest, '--profile', 'fast-dev', '--features', 'dev-smoke-binaries,qa-framebuffer-capture', '--bin', 'jump_mode_fixture'); Policy = 'verified-msvc-vctip' },
            @{ Name = 'fixture-tests'; Stage = 'test'; Executable = (Join-Path $script:WorkflowRoot 'target\fast-dev\deps\jump_mode_fixture-abc123.exe'); Arguments = @('--test-threads=1'); Policy = 'none' },
            @{ Name = 'fixture-generate'; Stage = 'test'; Executable = (Join-Path $script:WorkflowRoot 'target\fast-dev\jump_mode_fixture.exe'); Arguments = @('generate', $fixtureProfile, 'phase2-settings'); Policy = 'none' },
            @{ Name = 'fixture-verify'; Stage = 'test'; Executable = (Join-Path $script:WorkflowRoot 'target\fast-dev\jump_mode_fixture.exe'); Arguments = @('verify', $fixtureProfile, 'phase2-settings'); Policy = 'none' }
        )
        if ($SelectionName -ceq 'phase2-settings-native' -and $Outcome -cne 'environment') {
            $definitions += @{ Name = 'app-build'; Stage = 'compile'; Executable = $cargo; Arguments = @('build', '--manifest-path', $manifest, '--profile', 'fast-dev', '--features', 'dev-smoke-binaries,qa-framebuffer-capture', '--bin', 'multi_launcher'); Policy = 'verified-msvc-vctip' }
        }
        $summaries = [System.Collections.Generic.List[object]]::new()
        foreach ($definition in $definitions) {
            $phaseDirectory = Join-Path $measureRoot ([string]$definition.Name)
            [IO.Directory]::CreateDirectory($phaseDirectory) | Out-Null
            $measurementPath = Join-Path $phaseDirectory 'measurement.json'
            $rawMeasurement = [ordered]@{
                Succeeded = $true
                CleanupVerified = $true
                OutputDrainVerified = $true
                CleanupErrors = @()
                RecordingErrors = @()
                SourceSha = $sourceSha
                Stage = $definition.Stage
                Executable = $definition.Executable
                ArgumentList = @($definition.Arguments)
                AuxiliaryProcessPolicy = $definition.Policy
            }
            [IO.File]::WriteAllText($measurementPath, (ConvertTo-Json -InputObject $rawMeasurement -Depth 8), [Text.UTF8Encoding]::new($false))
            $summaries.Add([ordered]@{
                name = $definition.Name
                stage = $definition.Stage
                source_sha = $sourceSha
                executable = $definition.Executable
                arguments = @($definition.Arguments)
                status = 'PASS'
                cleanup_verified = $true
                output_drain_verified = $true
                measurement_path = $measurementPath
            })
        }
        $fixtureOnly = $SelectionName -ceq 'phase2-settings-fixture'
        $nativeEnvironmentNotRun = $Outcome -ceq 'environment'
        $cleanupUncertain = $Outcome -ceq 'cleanup-uncertain'
        $manifestStatus = if ($nativeEnvironmentNotRun) { 'NOT_RUN_ENVIRONMENT' } else { 'PASS' }
        $scenarioStatus = if ($fixtureOnly) { 'BLOCKED' } elseif ($nativeEnvironmentNotRun) { 'NOT_RUN_ENVIRONMENT' } else { 'PASS' }
        $captureCount = if ($fixtureOnly -or $nativeEnvironmentNotRun) { 0 } else { 3 }
        $manifest = [ordered]@{
            run_id = $runId
            repository_root = $script:WorkflowRoot
            source_head = $sourceSha
            status = $manifestStatus
            phase = if ($fixtureOnly -or $nativeEnvironmentNotRun) { 'complete' } else { 'phase2_complete' }
            capture = @{ count = $captureCount; captures = @() }
            executable = if ($captureCount -gt 0) { @{ pid = 424242; sha256 = ('a' * 64) } } else { $null }
            fixture_manifest_sha256 = ('b' * 64)
            build_receipt_identity = 'synthetic measurement-linked app build'
            command_measurements = @($summaries.ToArray())
            post_build_native_timeout_seconds = 300
            post_build_native_shutdown_reserve_seconds = 30
            post_build_native_started_at_utc = $null
            post_build_native_deadline_utc = $null
            post_build_native_finished_at_utc = $null
            post_build_native_deadline_reached = $false
        }
        if (!$fixtureOnly -and !$nativeEnvironmentNotRun) {
            $started = [DateTime]::UtcNow
            $manifest.post_build_native_started_at_utc = $started.ToString('O')
            $manifest.post_build_native_deadline_utc = $started.AddSeconds(300).ToString('O')
            $manifest.post_build_native_finished_at_utc = $started.AddSeconds(2).ToString('O')
        }
        $scenarios = @(
            @{ scenario_id = 'm13-fixture-validation'; status = 'PASS' },
            @{ scenario_id = 'm13-phase2-settings'; status = $scenarioStatus; captures = @() }
        )
        $lifecycle = @{
            post_cleanup_state = if ($cleanupUncertain) { 'cleanup_uncertain' } elseif ($captureCount -gt 0) { 'owned_tree_gone' } else { 'not_started' }
            owned_tree_verified = ($captureCount -gt 0 -and !$cleanupUncertain)
            process_output_drain_verified = ($captureCount -gt 0 -and !$cleanupUncertain)
            cleanup_issues = if ($cleanupUncertain) { @('synthetic cleanup uncertainty') } else { @() }
        }
        $manifestPath = Join-Path $harnessRoot 'manifest.json'
        $scenariosPath = Join-Path $harnessRoot 'scenarios.json'
        $lifecyclePath = Join-Path $harnessRoot 'process_lifecycle.json'
        [IO.File]::WriteAllText($manifestPath, (ConvertTo-Json -InputObject $manifest -Depth 24), [Text.UTF8Encoding]::new($false))
        [IO.File]::WriteAllText($scenariosPath, (ConvertTo-Json -InputObject $scenarios -Depth 12), [Text.UTF8Encoding]::new($false))
        [IO.File]::WriteAllText($lifecyclePath, (ConvertTo-Json -InputObject $lifecycle -Depth 12), [Text.UTF8Encoding]::new($false))
        $link = [ordered]@{
            schema_version = 1
            selection_name = $SelectionName
            selection_kind = if ($fixtureOnly) { 'fixture-only' } else { 'native' }
            scenario = 'Phase2'
            surface = 'Settings'
            run_id = $runId
            evidence_root = $harnessRoot
            manifest_path = $manifestPath
            scenarios_path = $scenariosPath
            lifecycle_path = $lifecyclePath
            status = $manifestStatus
            fixture_status = 'PASS'
            scenario_status = $scenarioStatus
            native_status = if ($fixtureOnly) { 'NOT_RUN' } elseif ($nativeEnvironmentNotRun) { 'NOT_RUN_ENVIRONMENT' } elseif ($cleanupUncertain) { 'PASS' } else { 'PASS' }
            capture_count = $captureCount
            post_build_native_timeout_seconds = if ($fixtureOnly) { 0 } else { 300 }
            post_build_native_deadline_reached = $false
            command_measurements = @($summaries.ToArray())
        }
        [IO.File]::WriteAllText($linkPath, (ConvertTo-Json -InputObject $link -Depth 24), [Text.UTF8Encoding]::new($false))
        return [pscustomobject]@{ LinkPath = $linkPath; RunId = $runId }
    }.GetNewClosure()

    $fixtureSmokeRun = New-WorkflowRequestFromCli -Action Run -SmokeSelectionName phase2-settings-fixture
    Assert-WorkflowEqual $fixtureSmokeRun.CacheState 'unknown' 'Smoke request does not infer a cache state'
    Assert-WorkflowEqual $fixtureSmokeRun.WorkKind 'unknown' 'Smoke request does not infer a work kind'
    $fixtureSmokeCalls = [System.Collections.Generic.List[string]]::new()
    $fixtureSmokeInvoker = {
        param($spec)
        $fixtureSmokeCalls.Add([string]$spec.Name)
        Assert-WorkflowEqual $spec.Name 'jump-smoke-run' 'Settings fixture child name'
        Assert-WorkflowEqual $spec.Stage 'native' 'Settings fixture outer process stage'
        Assert-WorkflowEqual $spec.TimeoutSeconds 3600 'Settings fixture outer budget'
        $null = & $makeSyntheticJumpRun $spec 'phase2-settings-fixture' 'fixture'
        return & $measurementFactory $spec ''
    }.GetNewClosure()
    $fixtureSmokeResult = Invoke-WorkflowAction -Request $fixtureSmokeRun -EvidenceDirectory $testRoot -ProcessInvoker $fixtureSmokeInvoker
    Assert-WorkflowEqual $fixtureSmokeResult.Status 'PASS' 'Synthetic Settings fixture run classification'
    Assert-WorkflowEqual $fixtureSmokeResult.Classification 'FIXTURE_PASS' 'Synthetic Settings fixture is not native acceptance'
    Assert-WorkflowEqual $fixtureSmokeResult.Smoke.NativeStatus 'NOT_RUN' 'Fixture-only result records native execution as not run'
    Assert-WorkflowSequence @($fixtureSmokeCalls) @('jump-smoke-run') 'Settings fixture executes one checked harness command'
    $tamperedSmokeLink = Get-Content -LiteralPath $fixtureSmokeResult.SmokeReceiptPath -Raw | ConvertFrom-Json -AsHashtable
    $tamperedSmokeLink.run_id = 'jump_m13_phase2_settings_wrong'
    [IO.File]::WriteAllText($fixtureSmokeResult.SmokeReceiptPath, (ConvertTo-Json -InputObject $tamperedSmokeLink -Depth 24), [Text.UTF8Encoding]::new($false))
    $tamperedSmokeReceipt = Test-WorkflowSmokeReceipt -Path $fixtureSmokeResult.SmokeReceiptPath -Request $fixtureSmokeRun
    Assert-WorkflowEqual $tamperedSmokeReceipt.Valid $false 'Mismatched harness run id is rejected'

    $environmentSmokeRun = New-WorkflowRequestFromCli -Action Run -SmokeSelectionName phase2-settings-native
    $environmentSmokeInvoker = {
        param($spec)
        $null = & $makeSyntheticJumpRun $spec 'phase2-settings-native' 'environment'
        return & $measurementFactory $spec ''
    }.GetNewClosure()
    $environmentSmokeResult = Invoke-WorkflowAction -Request $environmentSmokeRun -EvidenceDirectory $testRoot -ProcessInvoker $environmentSmokeInvoker
    Assert-WorkflowEqual $environmentSmokeResult.Status 'NOT_RUN_ENVIRONMENT' 'Native Settings environment block status'
    Assert-WorkflowEqual $environmentSmokeResult.Classification 'ENVIRONMENT_NATIVE_NOT_RUN' 'Environment block is not a native pass'
    $environmentSmokeLink = Get-Content -LiteralPath $environmentSmokeResult.SmokeReceiptPath -Raw | ConvertFrom-Json -AsHashtable
    Assert-WorkflowEqual @($environmentSmokeLink.command_measurements).Count 6 'Environment-blocked native smoke stops before app build'
    Assert-WorkflowEqual $environmentSmokeLink.command_measurements[-1].name 'fixture-verify' 'Environment-blocked native measurement prefix'

    $cleanupSmokeInvoker = {
        param($spec)
        $null = & $makeSyntheticJumpRun $spec 'phase2-settings-native' 'cleanup-uncertain'
        return & $measurementFactory $spec ''
    }.GetNewClosure()
    $cleanupSmokeResult = Invoke-WorkflowAction -Request $environmentSmokeRun -EvidenceDirectory $testRoot -ProcessInvoker $cleanupSmokeInvoker
    Assert-WorkflowEqual $cleanupSmokeResult.Status 'FAIL' 'Native cleanup uncertainty fails smoke run'
    Assert-WorkflowEqual $cleanupSmokeResult.Classification 'CLEANUP_UNCERTAINTY' 'Native cleanup uncertainty classification'

    $quickRunRequest = New-WorkflowRequest -Action Run -Mode quick-check
    $expectedQuickCommand = $quickRunRequest.Phases[0].Command.ArgumentList
    foreach ($quickCase in @(
        @{ Name = 'success'; ExitCode = 0; Succeeded = $true; TimedOut = $false; ExpectedStatus = 'PASS'; ExpectedExit = 0 },
        @{ Name = 'nonzero'; ExitCode = 7; Succeeded = $false; TimedOut = $false; ExpectedStatus = 'FAIL'; ExpectedExit = 7 },
        @{ Name = 'timeout'; ExitCode = 0; Succeeded = $false; TimedOut = $true; ExpectedStatus = 'FAIL'; ExpectedExit = 124 }
    )) {
        $quickCalls = [System.Collections.Generic.List[string]]::new()
        $quickInvoker = {
            param($spec)
            $quickCalls.Add([string]$spec.Name)
            Assert-WorkflowEqual $spec.Name 'build' 'Quick-check phase name'
            Assert-WorkflowEqual $spec.Stage 'compile' 'Quick-check stage'
            Assert-WorkflowEqual $spec.TimeoutSeconds 1800 'Quick-check build deadline'
            Assert-WorkflowSequence @($spec.Command.ArgumentList) $expectedQuickCommand 'Quick-check exact Cargo command'
            return & $measurementFactory $spec '' $quickCase.ExitCode $quickCase.Succeeded $quickCase.TimedOut $true $true
        }.GetNewClosure()
        $quickResult = Invoke-WorkflowAction -Request $quickRunRequest -EvidenceDirectory $testRoot -ProcessInvoker $quickInvoker
        Assert-WorkflowSequence @($quickCalls) @('build') "Quick-check $($quickCase.Name) executes only its build"
        Assert-WorkflowEqual $quickResult.Status $quickCase.ExpectedStatus "Quick-check $($quickCase.Name) status"
        Assert-WorkflowEqual $quickResult.ExitCode $quickCase.ExpectedExit "Quick-check $($quickCase.Name) exit propagation"
        Assert-WorkflowEqual $quickResult.Request.TargetKind 'lib' "Quick-check $($quickCase.Name) retains library target"
        Assert-WorkflowEqual $quickResult.Request.ModeKind 'build' "Quick-check $($quickCase.Name) retains build mode"
    }

    $archiveSource = Join-Path $testRoot 'short-source.zip'
    Set-Content -LiteralPath $archiveSource -Value 'portable fixture archive bytes' -NoNewline
    $longArchiveRoot = Join-Path ([IO.Path]::GetTempPath()) ('wfl-' + [guid]::NewGuid().ToString('N').Substring(0, 12))
    $longArchivePath = Join-Path $longArchiveRoot 'nextest-recording.zip'
    while ($longArchivePath.Length -lt 280) {
        $longArchiveRoot = Join-Path $longArchiveRoot ('segment-' + [guid]::NewGuid().ToString('N').Substring(0, 24))
        $longArchivePath = Join-Path $longArchiveRoot 'nextest-recording.zip'
    }
    try {
        [IO.Directory]::CreateDirectory((Get-WorkflowManagedFilePath -Path $longArchiveRoot)) | Out-Null
        $archivePublication = Copy-WorkflowArchive -SourcePath $archiveSource -DestinationPath $longArchivePath
        Assert-WorkflowEqual ($longArchivePath.Length -gt 260) $true 'Portable archive long durable path fixture'
        Assert-WorkflowEqual $archivePublication.ContentMatches $true 'Portable archive publication hash/length verification'
        Assert-WorkflowEqual $archivePublication.Source.Sha256 $archivePublication.Destination.Sha256 'Portable archive publication digest equality'
        Assert-WorkflowEqual $archivePublication.Source.ByteLength $archivePublication.Destination.ByteLength 'Portable archive publication byte length equality'
        $publishedIdentityBefore = Get-WorkflowArchiveIdentity -Path $longArchivePath
        Assert-WorkflowThrows { Copy-WorkflowArchive -SourcePath $archiveSource -DestinationPath $longArchivePath } 'Portable archive publication does not overwrite'
        $publishedIdentityAfter = Get-WorkflowArchiveIdentity -Path $longArchivePath
        Assert-WorkflowEqual $publishedIdentityAfter.Sha256 $publishedIdentityBefore.Sha256 'Existing archive retained after no-overwrite rejection'
    }
    finally {
        if (Test-Path -LiteralPath (Get-WorkflowManagedFilePath -Path $longArchiveRoot) -PathType Container) {
            [IO.Directory]::Delete((Get-WorkflowManagedFilePath -Path $longArchiveRoot), $true)
        }
    }

    $metadataPath = Join-Path $testRoot 'binaries metadata.json'
    Set-Content -LiteralPath $metadataPath -Value '{"rust-binaries":[{"binary-id":"multi_launcher::lib"}]}' -Encoding utf8
    $metadataHash = (Get-FileHash -LiteralPath $metadataPath -Algorithm SHA256).Hash.ToLowerInvariant()
    $discoveryRequest = New-WorkflowRequest -Action Discover -Mode fast-dev-tests -TargetKind lib
    $discoveryCalls = [System.Collections.Generic.List[string]]::new()
    $discoverInvoker = {
        param($spec)
        $discoveryCalls.Add([string]$spec.Name)
        $stdout = ''
        if ($spec.Name -eq 'nextest-version') { $stdout = 'cargo-nextest 0.9.135' }
        elseif ($spec.Name -eq 'target-prepare') { $stdout = '{"rust-binaries":[{"binary-id":"multi_launcher::lib"}]}' }
        elseif ($spec.Name.StartsWith('inventory-', [StringComparison]::Ordinal)) {
            $selectionName = $spec.Name.Substring('inventory-'.Length)
            $selection = @($spec.Request.Selections | Where-Object { [string]$_.name -ceq $selectionName })[0]
            $testName = [string]$selection.exactCase
            $stdout = ConvertTo-Json -Compress -Depth 8 -InputObject ([ordered]@{
                'rust-suites' = [ordered]@{
                    'multi_launcher::lib' = [ordered]@{
                        'binary-id' = 'multi_launcher::lib'
                        'package-name' = 'multi_launcher'
                        'binary-name' = 'multi_launcher'
                        kind = 'lib'
                        testcases = [ordered]@{
                            $testName = [ordered]@{ ignored = $false; 'filter-match' = [ordered]@{ status = 'matches' } }
                        }
                    }
                }
            })
        }
        elseif ($spec.Name -eq 'selected-run') { $stdout = $fixturePassRunOutput }
        elseif ($spec.Name -eq 'record-store-list') { $stdout = $fixtureRunId }
        elseif ($spec.Name -eq 'record-store-export') {
            $archiveIndex = [Array]::IndexOf([string[]]$spec.Command.ArgumentList, '--archive-file')
            if ($archiveIndex -lt 0) { throw 'Store export omitted --archive-file.' }
            Set-Content -LiteralPath $spec.Command.ArgumentList[$archiveIndex + 1] -Value 'portable-fixture-recording' -NoNewline
            $stdout = 'exported'
        }
        else { throw "Unexpected mocked workflow phase '$($spec.Name)'." }
        return & $measurementFactory $spec $stdout
    }.GetNewClosure()
    $discoveryResult = Invoke-WorkflowAction -Request $discoveryRequest -EvidenceDirectory $testRoot -ProcessInvoker $discoverInvoker
    Assert-WorkflowEqual $discoveryResult.Status 'PASS' 'Mocked Discover action'
    Assert-WorkflowEqual $discoveryResult.Discovery.selections.Count 5 'Actual Discover writer selection count'
    Assert-WorkflowEqual $discoveryCalls.Count 7 'Mocked Discover stage count'
    $receiptPath = [string]$discoveryResult.DiscoveryReceiptPath
    Assert-WorkflowEqual (Test-Path -LiteralPath $receiptPath -PathType Leaf) $true 'Actual Discover receipt persisted'
    $validReceipt = Get-Content -LiteralPath $receiptPath -Raw -ErrorAction Stop | ConvertFrom-Json -AsHashtable -ErrorAction Stop
    $receiptStatus = Test-WorkflowDiscoveryReceipt -Path $receiptPath -Request $runRequest
    Assert-WorkflowEqual $receiptStatus.Status 'PASS' 'Matching discovery receipt'
    $metadataPath = [string]$receiptStatus.BinariesMetadataPath
    $metadataHash = (Get-FileHash -LiteralPath $metadataPath -Algorithm SHA256).Hash.ToLowerInvariant()

    $selection = $runRequest.Selection
    $identity = [ordered]@{
        binaryId = 'multi_launcher::lib'
        packageName = [string]$selection.package
        binaryName = [string]$selection.targetName
        kind = [string]$selection.nextestKind
        testName = [string]$selection.exactCase
    }

    $previewRequest = New-WorkflowRequestFromCli -Action Preview -Mode fast-dev-tests -SelectionName tables `
        -DiscoveryReceipt $receiptPath -RunnerProfile 'ci-fast' -TestArguments $runArguments
    Assert-WorkflowEqual $previewRequest.Status 'RESOLVED' 'Test preview resolution'
    $previewResult = Invoke-WorkflowAction -Request $previewRequest -ProcessInvoker { throw 'Preview launched a child process.' }
    Assert-WorkflowEqual $previewResult.Status 'PREVIEW' 'Non-executing test preview'

    $publicFacadePath = Join-Path $script:WorkflowRoot 'dev\workflows\run.ps1'
    $powershellPath = (Get-Command pwsh.exe -CommandType Application -ErrorAction Stop | Select-Object -First 1).Source
    $publicArguments = @(
        '-NoProfile', '-File', $publicFacadePath,
        '-Action', 'Preview', '-Mode', 'fast-dev-tests', '-SelectionName', 'tables',
        '-DiscoveryReceipt', $receiptPath
    )
    $publicMeasurement = & (Join-Path $script:WorkflowRoot 'dev\track_b\measure.ps1') `
        -Executable $powershellPath -ArgumentList $publicArguments -ScenarioId 'workflow-public-preview-fixture' `
        -EvidenceDirectory $testRoot -WorkingDirectory $script:WorkflowRoot -SourceRoot $script:WorkflowRoot `
        -Profile 'workflow-fixture' -Stage provenance -TimeoutSeconds 20 -CleanupTimeoutSeconds 5
    if (!$publicMeasurement.Succeeded) {
        throw "Public workflow preview failed through the checked owner: $($publicMeasurement.ExecutionReason), exit $($publicMeasurement.ExitCode)."
    }
    $publicPreview = Get-Content -LiteralPath $publicMeasurement.StdoutPath -Raw -ErrorAction Stop | ConvertFrom-Json -ErrorAction Stop
    Assert-WorkflowEqual $publicPreview.Status 'PREVIEW' 'Public facade preview without target kind'
    Assert-WorkflowEqual $publicPreview.Request.SelectionName 'tables' 'Public facade selection'
    Assert-WorkflowEqual $publicPreview.Request.TargetKind 'lib' 'Public facade target inferred from selection'
    Set-WorkflowRequestMetadataPath -Request $runRequest -Path $metadataPath
    Assert-WorkflowSequence @($previewRequest.RunCommand.ArgumentList) @($runRequest.RunCommand.ArgumentList) 'Preview and run request agreement'

    $malformedPath = Join-Path $testRoot 'malformed.json'
    Set-Content -LiteralPath $malformedPath -Value '{invalid-json' -Encoding utf8
    Assert-WorkflowEqual (Test-WorkflowDiscoveryReceipt -Path $malformedPath -Request $runRequest).Status 'STALE_DISCOVERY' 'Malformed receipt rejection'
    $staleReceipt = ConvertFrom-Json -InputObject (ConvertTo-Json -InputObject $validReceipt -Depth 8) -AsHashtable
    $staleReceipt.catalogSha256 = '0000000000000000000000000000000000000000000000000000000000000000'
    $stalePath = Join-Path $testRoot 'stale.json'
    Set-Content -LiteralPath $stalePath -Value (ConvertTo-Json -InputObject $staleReceipt -Depth 8) -Encoding utf8
    Assert-WorkflowEqual (Test-WorkflowDiscoveryReceipt -Path $stalePath -Request $runRequest).Status 'STALE_DISCOVERY' 'Stale receipt rejection'
    $zeroReceipt = ConvertFrom-Json -InputObject (ConvertTo-Json -InputObject $validReceipt -Depth 8) -AsHashtable
    $zeroReceipt.selections = @([ordered]@{ name = 'tables'; status = 'PASS'; selectedCaseCount = 0; selectedIdentity = $identity })
    $zeroPath = Join-Path $testRoot 'zero.json'
    Set-Content -LiteralPath $zeroPath -Value (ConvertTo-Json -InputObject $zeroReceipt -Depth 8) -Encoding utf8
    Assert-WorkflowEqual (Test-WorkflowDiscoveryReceipt -Path $zeroPath -Request $runRequest).Status 'STALE_DISCOVERY' 'Zero-case receipt rejection'

    $calls = [System.Collections.Generic.List[string]]::new()
    $successInvoker = {
        param($spec)
        $calls.Add([string]$spec.Name)
        $stdout = ''
        if ($spec.Name -eq 'nextest-version') { $stdout = 'cargo-nextest 0.9.135' }
        elseif ($spec.Name -eq 'target-prepare') { $stdout = '{"rust-binaries":[{"binary-id":"multi_launcher::lib"}]}' }
        elseif ($spec.Name -eq 'selected-inventory') {
            $name = [string]$spec.Request.Selection.exactCase
            $stdout = ConvertTo-Json -Compress -Depth 8 -InputObject ([ordered]@{
                'rust-suites' = [ordered]@{
                    'multi_launcher::lib' = [ordered]@{
                        'binary-id' = 'multi_launcher::lib'
                        'package-name' = 'multi_launcher'
                        'binary-name' = 'multi_launcher'
                        kind = 'lib'
                        testcases = [ordered]@{ $name = [ordered]@{ ignored = $false; 'filter-match' = [ordered]@{ status = 'matches' } } }
                    }
                }
            })
        }
        elseif ($spec.Name -eq 'selected-run') { $stdout = $fixturePassRunOutput }
        elseif ($spec.Name -eq 'record-store-list') { $stdout = "recorded run $fixtureRunId" }
        elseif ($spec.Name -eq 'record-store-export') {
            $archiveIndex = [Array]::IndexOf([string[]]$spec.Command.ArgumentList, '--archive-file')
            if ($archiveIndex -lt 0) { throw 'Store export omitted --archive-file.' }
            Set-Content -LiteralPath $spec.Command.ArgumentList[$archiveIndex + 1] -Value 'portable-fixture-recording' -NoNewline
            $stdout = 'exported'
        }
        return & $measurementFactory $spec $stdout
    }.GetNewClosure()
    $runResult = Invoke-WorkflowAction -Request $runRequest -EvidenceDirectory $testRoot -ProcessInvoker $successInvoker
    if ($runResult.Status -ne 'PASS') { $runResult | ConvertTo-Json -Depth 12 | Write-Host }
    Assert-WorkflowEqual $runResult.Status 'PASS' 'Mocked selected run'
    Assert-WorkflowEqual $runResult.ExitCode 0 'Mocked selected run exit'
    Assert-WorkflowSequence @($calls) @('nextest-version', 'target-prepare', 'selected-inventory', 'selected-run', 'record-store-list', 'record-store-export') 'Owned phase order'
    Assert-WorkflowEqual (Test-Path -LiteralPath $runResult.ReceiptPath -PathType Leaf) $true 'Workflow receipt persisted'
    Assert-WorkflowEqual $runResult.Phases.Count 6 'Workflow phase receipts'
    Assert-WorkflowEqual $runResult.Results.Complete $true 'Structured selected results complete'
    Assert-WorkflowEqual $runResult.Results.Passed 1 'Structured selected pass count'
    Assert-WorkflowEqual $runResult.Recording.Status 'PASS' 'Portable run recording published'
    Assert-WorkflowEqual $runResult.Recording.PublicationVerified $true 'Durable recording publication is byte-verified'
    Assert-WorkflowEqual $runResult.Recording.NativeStaging.PathScope 'private Nextest state' 'Native archive stays scoped to private staging'
    Assert-WorkflowEqual $runResult.Recording.NativeStaging.FileName 'recording-export.zip' 'Native archive staging filename'
    Assert-WorkflowEqual $runResult.Recording.NativeStaging.Sha256 $runResult.Recording.ArchiveSha256 'Native and durable recording digests agree'
    Assert-WorkflowEqual $runResult.Recording.NativeStaging.ByteLength $runResult.Recording.ArchiveByteLength 'Native and durable recording lengths agree'
    Assert-WorkflowEqual $runResult.Recording.NativeStaging.CleanupVerified $true 'Native recording staging removed with private state'
    Assert-WorkflowEqual (Test-Path -LiteralPath (Join-Path $runResult.RecordingContext.StateDirectory 'recording-export.zip')) $false 'Native staging file removed after publication'
    Assert-WorkflowEqual $runResult.RecordingStorageCleanupVerified $true 'Private Nextest state cleanup'

    $throwingInvoker = {
        param($spec)
        if ($spec.Name -eq 'nextest-version') { return & $measurementFactory $spec 'cargo-nextest 0.9.135' }
        if ($spec.Name -eq 'target-prepare') { return & $measurementFactory $spec '{"rust-binaries":[{"binary-id":"multi_launcher::lib"}]}' }
        if ($spec.Name -eq 'selected-inventory') {
            $name = [string]$spec.Request.Selection.exactCase
            $stdout = ConvertTo-Json -Compress -Depth 8 -InputObject ([ordered]@{
                'rust-suites' = [ordered]@{ 'multi_launcher::lib' = [ordered]@{
                    'binary-id' = 'multi_launcher::lib'; 'package-name' = 'multi_launcher'; 'binary-name' = 'multi_launcher'; kind = 'lib'
                    testcases = [ordered]@{ $name = [ordered]@{ ignored = $false; 'filter-match' = [ordered]@{ status = 'matches' } } }
                } }
            })
            return & $measurementFactory $spec $stdout
        }
        if ($spec.Name -eq 'selected-run') { throw 'injected selected-run invocation failure' }
        throw "Unexpected mock phase $($spec.Name)"
    }.GetNewClosure()
    $throwingRequest = New-WorkflowRequest -Action Run -Mode fast-dev-tests -SelectionName tables -RunnerProfile 'ci-fast' -TestArguments $runArguments
    $throwingRun = Invoke-WorkflowAction -Request $throwingRequest -EvidenceDirectory $testRoot -ProcessInvoker $throwingInvoker
    Assert-WorkflowEqual $throwingRun.Status 'FAIL' 'Selected-run invocation exception remains a failure'
    Assert-WorkflowEqual $throwingRun.Error 'injected selected-run invocation failure' 'Original invocation exception is not hidden by cleanup'
    Assert-WorkflowEqual $throwingRun.RecordingStorageCleanupVerified $true 'Recording state cleanup runs after invocation exception'
    Assert-WorkflowEqual (Test-Path -LiteralPath $throwingRun.RecordingContext.StateDirectory) $false 'Recording state directory removed without garbage collection'
    Assert-WorkflowEqual (Test-Path -LiteralPath $throwingRun.ReceiptPath -PathType Leaf) $true 'Failure and cleanup receipt persisted after invocation exception'
    $throwingReceipt = Get-Content -LiteralPath $throwingRun.ReceiptPath -Raw -ErrorAction Stop | ConvertFrom-Json -ErrorAction Stop
    Assert-WorkflowEqual $throwingReceipt.Error 'injected selected-run invocation failure' 'Persisted receipt retains original invocation exception'
    Assert-WorkflowEqual $throwingReceipt.RecordingStorageCleanupVerified $true 'Persisted receipt confirms state cleanup'

    $capturedFailureInvoker = {
        param($spec)
        if ($spec.Name -eq 'selected-run') { return & $measurementFactory $spec $fixtureFailRunOutput 7 $false }
        if ($spec.Name -eq 'record-store-list') { return & $measurementFactory $spec $fixtureRunId }
        if ($spec.Name -eq 'record-store-export') {
            $archiveIndex = [Array]::IndexOf([string[]]$spec.Command.ArgumentList, '--archive-file')
            Set-Content -LiteralPath $spec.Command.ArgumentList[$archiveIndex + 1] -Value 'portable-fixture-recording' -NoNewline
            return & $measurementFactory $spec 'exported'
        }
        if ($spec.Name -eq 'selected-inventory') {
            $name = [string]$spec.Request.Selection.exactCase
            $stdout = ConvertTo-Json -Compress -Depth 8 -InputObject ([ordered]@{
                'rust-suites' = [ordered]@{ 'multi_launcher::lib' = [ordered]@{
                    'binary-id' = 'multi_launcher::lib'; 'package-name' = 'multi_launcher'; 'binary-name' = 'multi_launcher'; kind = 'lib'
                    testcases = [ordered]@{ $name = [ordered]@{ ignored = $false; 'filter-match' = [ordered]@{ status = 'matches' } } }
                } }
            })
            return & $measurementFactory $spec $stdout
        }
        $stdout = if ($spec.Name -eq 'nextest-version') { 'cargo-nextest 0.9.135' } elseif ($spec.Name -eq 'target-prepare') { '{"rust-binaries":[{"binary-id":"multi_launcher::lib"}]}' } else { throw "Unexpected mock phase $($spec.Name)" }
        return & $measurementFactory $spec $stdout
    }.GetNewClosure()
    $failedRequest = New-WorkflowRequest -Action Run -Mode fast-dev-tests -SelectionName tables -RunnerProfile 'ci-fast' -TestArguments $runArguments
    $failedRun = Invoke-WorkflowAction -Request $failedRequest -EvidenceDirectory $testRoot -ProcessInvoker $capturedFailureInvoker
    Assert-WorkflowEqual $failedRun.Status 'FAIL' 'Selected test failure is retained'
    Assert-WorkflowEqual $failedRun.ExitCode 7 'Selected test native exit is retained'
    Assert-WorkflowEqual $failedRun.Results.Failed 1 'Structured failed count'
    Assert-WorkflowEqual $failedRun.Results.ConfirmedFailures.Count 1 'Confirmed failed identity count'
    Assert-WorkflowEqual $failedRun.Recording.Status 'PASS' 'Failure recording exported'
    $firstFailureHash = (Get-FileHash -LiteralPath $failedRun.ReceiptPath -Algorithm SHA256).Hash
    $replayRequest = New-WorkflowRequestFromCli -Action Replay -ReplayReceipt $failedRun.ReceiptPath
    Assert-WorkflowEqual $replayRequest.Action 'Replay' 'Replay request resolution'
    Assert-WorkflowEqual $replayRequest.RunCommand.ArgumentList[[Array]::IndexOf([string[]]$replayRequest.RunCommand.ArgumentList, '-R') + 1] $failedRun.Recording.ArchivePath 'Replay uses exact original recording'
    Assert-WorkflowEqual $replayRequest.RunnerProfile 'ci-fast' 'Replay runner profile preserved'
    Assert-WorkflowSequence @($replayRequest.TestArguments) $runArguments 'Replay test arguments preserved'
    Assert-WorkflowEqual $replayRequest.ReplaySource.SourceChanged $false 'Unchanged source replay disclosure'
    $replayFilterIndex = [Array]::IndexOf([string[]]$replayRequest.RunCommand.ArgumentList, '-E')
    $replayFilter = [string]$replayRequest.RunCommand.ArgumentList[$replayFilterIndex + 1]
    if (!$replayFilter.Contains('binary_id(') -or !$replayFilter.Contains('table_utility::tests::table_parse_csv_quotes_escaped_quotes_and_embedded_newlines_preserve_strings')) {
        throw "Replay filter did not retain exact failed binary/case identity: $replayFilter"
    }
    Assert-WorkflowEqual (Get-FileHash -LiteralPath $failedRun.ReceiptPath -Algorithm SHA256).Hash $firstFailureHash 'Original failure receipt remains immutable'

    $changedSourceReceipt = ConvertFrom-Json -InputObject (ConvertTo-Json -InputObject $failedRun -Depth 18) -AsHashtable
    $changedSourceReceipt.Request.SourceFingerprintSha256 = ('a' * 64)
    $changedSourcePath = Join-Path $testRoot 'changed-source-failed-run.json'
    Set-Content -LiteralPath $changedSourcePath -Value (ConvertTo-Json -InputObject $changedSourceReceipt -Depth 18) -Encoding utf8
    $changedSourceReplay = New-WorkflowRequestFromCli -Action Replay -ReplayReceipt $changedSourcePath
    Assert-WorkflowEqual $changedSourceReplay.ReplaySource.SourceChanged $true 'Changed source replay is explicitly disclosed'
    Assert-WorkflowEqual $changedSourceReplay.ReplaySource.OriginalSourceFingerprintSha256 ('a' * 64) 'Original source fingerprint retained'
    Assert-WorkflowEqual $changedSourceReplay.ReplaySource.FailedIdentities[0].BinaryId $failedRun.SelectedIdentity.BinaryId 'Failed receipt binary identity retained'
    Assert-WorkflowEqual $changedSourceReplay.ReplaySource.FailedIdentities[0].TestName $failedRun.SelectedIdentity.TestName 'Failed receipt exact test identity retained'
    $replayCalls = [System.Collections.Generic.List[string]]::new()
    $replayRuntimePaths = [System.Collections.Generic.List[string]]::new()
    $replayInvoker = {
        param($spec)
        $replayCalls.Add([string]$spec.Name)
        $stdout = ''
        if ($spec.Name -eq 'nextest-version') { $stdout = 'cargo-nextest 0.9.135' }
        elseif ($spec.Name -eq 'target-prepare') { $stdout = '{"rust-binaries":[{"binary-id":"multi_launcher::lib"}]}' }
        elseif ($spec.Name -eq 'selected-inventory') {
            $name = [string]$spec.Request.Selection.exactCase
            $stdout = ConvertTo-Json -Compress -Depth 8 -InputObject ([ordered]@{
                'rust-suites' = [ordered]@{ 'multi_launcher::lib' = [ordered]@{
                    'binary-id' = 'multi_launcher::lib'; 'package-name' = 'multi_launcher'; 'binary-name' = 'multi_launcher'; kind = 'lib'
                    testcases = [ordered]@{ $name = [ordered]@{ ignored = $false; 'filter-match' = [ordered]@{ status = 'matches' } } }
                } }
            })
        }
        elseif ($spec.Name -eq 'selected-run') {
            $replayIndex = [Array]::IndexOf([string[]]$spec.Command.ArgumentList, '-R')
            if ($replayIndex -lt 0) { throw 'Replay omitted its portable recording argument.' }
            $runtimePath = [string]$spec.Command.ArgumentList[$replayIndex + 1]
            if ([IO.Path]::GetFileName($runtimePath) -cne 'replay-input.zip' -or
                !$runtimePath.StartsWith([IO.Path]::GetFullPath([IO.Path]::GetTempPath()), [StringComparison]::OrdinalIgnoreCase)) {
                throw 'Replay did not pass a private short-path staged recording.'
            }
            $runtimeHash = (Get-FileHash -LiteralPath $runtimePath -Algorithm SHA256 -ErrorAction Stop).Hash.ToLowerInvariant()
            $runtimeLength = (Get-Item -LiteralPath $runtimePath -ErrorAction Stop).Length
            if ($runtimeHash -cne $failedRun.Recording.ArchiveSha256 -or $runtimeLength -ne $failedRun.Recording.ArchiveByteLength) {
                throw 'Replay staging changed the original archive bytes.'
            }
            $replayRuntimePaths.Add($runtimePath)
            $filterIndex = [Array]::IndexOf([string[]]$spec.Command.ArgumentList, '-E')
            $filter = [string]$spec.Command.ArgumentList[$filterIndex + 1]
            if (!$filter.Contains('binary_id(') -or !$filter.Contains('table_utility::tests::table_parse_csv_quotes_escaped_quotes_and_embedded_newlines_preserve_strings')) { throw 'Replay did not pass the exact failed binary/case filter.' }
            $stdout = $fixturePassRunOutput
        }
        elseif ($spec.Name -eq 'record-store-list') { $stdout = "recorded run $fixtureRunId" }
        elseif ($spec.Name -eq 'record-store-export') {
            $archiveIndex = [Array]::IndexOf([string[]]$spec.Command.ArgumentList, '--archive-file')
            if ($archiveIndex -lt 0) { throw 'Replay store export omitted --archive-file.' }
            Set-Content -LiteralPath $spec.Command.ArgumentList[$archiveIndex + 1] -Value 'portable-replay-fixture-recording' -NoNewline
            $stdout = 'exported'
        }
        else { throw "Unexpected mocked replay phase '$($spec.Name)'." }
        return & $measurementFactory $spec $stdout
    }.GetNewClosure()
    $replayed = Invoke-WorkflowAction -Request $changedSourceReplay -EvidenceDirectory $testRoot -ProcessInvoker $replayInvoker
    Assert-WorkflowEqual $replayed.Status 'PASS' 'Synthetic changed-source replay completes after fresh preparation and inventory'
    Assert-WorkflowEqual $replayed.SelectedIdentity.BinaryId $failedRun.SelectedIdentity.BinaryId 'Fresh replay inventory revalidates binary identity'
    Assert-WorkflowEqual $replayed.SelectedIdentity.TestName $failedRun.SelectedIdentity.TestName 'Fresh replay inventory revalidates exact case'
    Assert-WorkflowEqual $replayed.ReplaySource.SourceChanged $true 'Changed source disclosure reaches result receipt'
    Assert-WorkflowEqual $replayed.ReplaySource.RecordingPath $failedRun.Recording.ArchivePath 'Replay preserves immutable durable recording provenance'
    Assert-WorkflowEqual $replayed.ReplaySource.RuntimeRecording.Sha256 $failedRun.Recording.ArchiveSha256 'Replay receipt records private staged archive hash'
    Assert-WorkflowEqual $replayed.ReplaySource.RuntimeRecording.CleanupVerified $true 'Replay staging cleanup is verified'
    Assert-WorkflowEqual $replayed.RecordingContext.ReplayStaging.CleanupVerified $true 'Replay stage cleanup is recorded in context'
    Assert-WorkflowEqual $replayRuntimePaths.Count 1 'Replay consumed exactly one staged archive'
    Assert-WorkflowEqual (Test-Path -LiteralPath $replayRuntimePaths[0]) $false 'Replay archive staging removed after execution'
    Assert-WorkflowSequence @($replayCalls) @('nextest-version', 'target-prepare', 'selected-inventory', 'selected-run', 'record-store-list', 'record-store-export') 'Replay owns fresh validation and exact archived execution'
    $staleReplayReceipt = ConvertFrom-Json -InputObject (ConvertTo-Json -InputObject $failedRun -Depth 18) -AsHashtable
    $staleReplayReceipt.Request.CatalogSha256 = ('0' * 64)
    $staleReplayPath = Join-Path $testRoot 'stale-catalog-failed-run.json'
    Set-Content -LiteralPath $staleReplayPath -Value (ConvertTo-Json -InputObject $staleReplayReceipt -Depth 18) -Encoding utf8
    Assert-WorkflowThrows { New-WorkflowRequestFromCli -Action Replay -ReplayReceipt $staleReplayPath } 'Stale catalog replay rejection'

    $failureCalls = [System.Collections.Generic.List[string]]::new()
    $versionFailureInvoker = {
        param($spec)
        $failureCalls.Add([string]$spec.Name)
        return & $measurementFactory $spec 'nextest unavailable' 7 $false
    }.GetNewClosure()
    $failureRequest = New-WorkflowRequest -Action Run -Mode fast-dev-tests -SelectionName tables
    $failureResult = Invoke-WorkflowAction -Request $failureRequest -EvidenceDirectory $testRoot -ProcessInvoker $versionFailureInvoker
    Assert-WorkflowEqual $failureResult.Status 'FAIL' 'Nextest failure status'
    Assert-WorkflowEqual $failureResult.ExitCode 7 'Native exit code propagation'
    Assert-WorkflowSequence @($failureCalls) @('nextest-version') 'Failure stops before target preparation'

    $timeoutCalls = [System.Collections.Generic.List[string]]::new()
    $timeoutInvoker = {
        param($spec)
        $timeoutCalls.Add([string]$spec.Name)
        if ($spec.Name -eq 'target-prepare') {
            return & $measurementFactory $spec '' 0 $false $true $false $false
        }
        return & $measurementFactory $spec 'cargo-nextest 0.9.135'
    }.GetNewClosure()
    $timeoutRequest = New-WorkflowRequest -Action Run -Mode fast-dev-tests -SelectionName tables
    $timeoutResult = Invoke-WorkflowAction -Request $timeoutRequest -EvidenceDirectory $testRoot -ProcessInvoker $timeoutInvoker
    Assert-WorkflowEqual $timeoutResult.Status 'FAIL' 'Uncertain cleanup status'
    Assert-WorkflowEqual $timeoutResult.ExitCode 124 'Timeout result'
    Assert-WorkflowSequence @($timeoutCalls) @('nextest-version', 'target-prepare') 'Timeout stops before inventory and execution'
    Assert-WorkflowEqual $timeoutResult.Phases[1].CleanupVerified $false 'Cleanup uncertainty preserved in receipt'
    Assert-WorkflowEqual $timeoutResult.Phases[1].OutputDrainVerified $false 'Output-drain uncertainty preserved in receipt'
}
finally {
    if (Test-Path -LiteralPath $testRoot -PathType Container) {
        $tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd([char[]]@('\', '/')) + [IO.Path]::DirectorySeparatorChar
        $fullTestRoot = [IO.Path]::GetFullPath($testRoot)
        if ($fullTestRoot.StartsWith($tempRoot, [StringComparison]::OrdinalIgnoreCase)) {
            [IO.Directory]::Delete($fullTestRoot, $true)
        }
    }
}

Write-Host 'Workflow catalog, resolver, preview, compatibility, and failure-boundary fixtures passed.'
