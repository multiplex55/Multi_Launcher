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

$cargoWrapper = Join-Path $script:WorkflowRoot 'dev\track_b\cargo.ps1'
Assert-WorkflowThrows { & $cargoWrapper -Preset quick-check -CargoArguments @('--all-targets') -PrintCommand } 'Long selector conflict'
Assert-WorkflowThrows { & $cargoWrapper -Preset full-debug -CargoArguments @('--package=other') -PrintCommand } 'Equals selector conflict'
Assert-WorkflowThrows { & $cargoWrapper -Preset full-debug -CargoArguments @('-pother') -PrintCommand } 'Short selector conflict'

$runArguments = @('--nocapture', 'quoted "argument" with spaces', 'filter!:punctuation')
$runRequest = New-WorkflowRequest -Action Run -Mode fast-dev-tests -SelectionName tables -RunnerProfile 'ci-fast' -TestArguments $runArguments
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
Assert-WorkflowEqual $runRequest.RunCommand.ArgumentList[[Array]::IndexOf([string[]]$runRequest.RunCommand.ArgumentList, '-P') + 1] 'ci-fast' 'Nextest profile argument'
$punctuationFilter = Get-WorkflowFilterExpression -ExactCase 'case::with/slash "quote" and punctuation!?'
if (!$punctuationFilter.Contains('\/') -or !$punctuationFilter.Contains('\?') -or !$punctuationFilter.Contains('"quote"')) {
    throw "Exact filter did not safely preserve regex delimiters and punctuation: $punctuationFilter"
}
Assert-WorkflowThrows { New-WorkflowRequest -Action Run -Mode fast-dev-tests -SelectionName unknown } 'Unknown catalog selection'
Assert-WorkflowThrows { New-WorkflowRequest -Action Run -Mode fast-dev-tests -SelectionName tables -TargetKind integration } 'Conflicting selection target'

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
try {
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
        else { throw "Unexpected mocked discovery phase '$($spec.Name)'." }
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
        return & $measurementFactory $spec $stdout
    }.GetNewClosure()
    $runResult = Invoke-WorkflowAction -Request $runRequest -EvidenceDirectory $testRoot -ProcessInvoker $successInvoker
    if ($runResult.Status -ne 'PASS') { $runResult | ConvertTo-Json -Depth 12 | Write-Host }
    Assert-WorkflowEqual $runResult.Status 'PASS' 'Mocked selected run'
    Assert-WorkflowEqual $runResult.ExitCode 0 'Mocked selected run exit'
    Assert-WorkflowSequence @($calls) @('nextest-version', 'target-prepare', 'selected-inventory', 'selected-run') 'Owned phase order'
    Assert-WorkflowEqual (Test-Path -LiteralPath $runResult.ReceiptPath -PathType Leaf) $true 'Workflow receipt persisted'
    Assert-WorkflowEqual $runResult.Phases.Count 4 'Workflow phase receipts'

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
