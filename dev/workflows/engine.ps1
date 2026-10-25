# Shared resolver and bounded workflow action boundary. Dot-source this file from
# run.ps1 or the focused self-test; child processes always go through measure.ps1.

$script:WorkflowRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..\..') -ErrorAction Stop).ProviderPath
$script:WorkflowCatalogPath = Join-Path $PSScriptRoot 'catalog.json'
$script:WorkflowCargoPath = Join-Path $script:WorkflowRoot 'dev\track_b\cargo.ps1'
$script:WorkflowMeasurePath = Join-Path $script:WorkflowRoot 'dev\track_b\measure.ps1'
$script:WorkflowJumpHarnessPath = Join-Path $script:WorkflowRoot 'dev\jump_mode\run.ps1'
$script:WorkflowCommonPath = Join-Path $script:WorkflowRoot 'dev\test_perf\common.ps1'
. $script:WorkflowCommonPath
. (Join-Path $PSScriptRoot 'results.ps1')

function Get-WorkflowCatalog {
    $catalog = Get-Content -LiteralPath $script:WorkflowCatalogPath -Raw -ErrorAction Stop |
        ConvertFrom-Json -AsHashtable -ErrorAction Stop
    if ($catalog.schemaVersion -ne 1 -or $catalog.package -cne 'multi_launcher') {
        throw 'Workflow catalog schema or package identity is not supported.'
    }
    return $catalog
}

function Get-WorkflowCatalogSha256 {
    return (Get-FileHash -LiteralPath $script:WorkflowCatalogPath -Algorithm SHA256 -ErrorAction Stop).Hash.ToLowerInvariant()
}

function Get-WorkflowSourceFingerprint {
    $files = [System.Collections.Generic.List[string]]::new()
    foreach ($directory in @('src', 'tests')) {
        $directoryPath = Join-Path $script:WorkflowRoot $directory
        if (!(Test-Path -LiteralPath $directoryPath -PathType Container)) { throw "Workflow source directory is missing: $directory" }
        foreach ($file in Get-ChildItem -LiteralPath $directoryPath -Filter '*.rs' -File -Recurse -ErrorAction Stop) {
            $files.Add($file.FullName)
        }
    }
    foreach ($relativePath in @(
        'Cargo.toml', 'Cargo.lock', 'build.rs', '.cargo/config.toml',
        'dev/track_b/cargo.ps1', 'dev/track_b/measure.ps1', 'dev/track_b/comparison.ps1', 'dev/track_b/compare_measurements.ps1',
        'dev/workflows/owned_process.ps1', 'dev/workflows/engine.ps1',
        'dev/workflows/run.ps1', 'dev/workflows/results.ps1', 'dev/workflows/verify_msvc_auxiliary.ps1', 'dev/workflows/catalog.json',
        'dev/jump_mode/run.ps1',
        'dev/test_perf/common.ps1'
    )) {
        $path = Join-Path $script:WorkflowRoot $relativePath
        if (Test-Path -LiteralPath $path -PathType Leaf) { $files.Add($path) }
    }

    $entries = [System.Collections.Generic.List[string]]::new()
    foreach ($path in @($files | Sort-Object -Unique)) {
        $relativePath = [IO.Path]::GetRelativePath($script:WorkflowRoot, $path).Replace('\', '/')
        $hash = (Get-FileHash -LiteralPath $path -Algorithm SHA256 -ErrorAction Stop).Hash.ToLowerInvariant()
        $entries.Add("$relativePath=$hash")
    }
    $bytes = [Text.UTF8Encoding]::new($false).GetBytes(($entries -join "`n"))
    return [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($bytes)).ToLowerInvariant()
}

function Get-WorkflowCargoPath {
    $application = Get-Command -Name 'cargo.exe' -CommandType Application -ErrorAction Stop | Select-Object -First 1
    if ($null -eq $application) { throw 'cargo.exe was not found as a native application on PATH.' }
    $path = if ($application.Source) { $application.Source } else { $application.Path }
    return [IO.Path]::GetFullPath($path)
}

function Get-WorkflowNextestIdentity {
    $application = Get-Command -Name 'cargo-nextest.exe' -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($null -eq $application) { throw 'cargo-nextest.exe was not found on PATH; fast-dev-tests requires Cargo Nextest.' }
    $path = if ($application.Source) { $application.Source } else { $application.Path }
    $resolvedPath = [IO.Path]::GetFullPath($path)
    return [pscustomobject][ordered]@{
        Path = $resolvedPath
        Sha256 = (Get-FileHash -LiteralPath $resolvedPath -Algorithm SHA256 -ErrorAction Stop).Hash.ToLowerInvariant()
    }
}

function Get-WorkflowPowerShellPath {
    $application = Get-Command -Name 'pwsh.exe' -CommandType Application -ErrorAction Stop | Select-Object -First 1
    if ($null -eq $application) { throw 'pwsh.exe was not found as a native application on PATH.' }
    $path = if ($application.Source) { $application.Source } else { $application.Path }
    return [IO.Path]::GetFullPath($path)
}

function Get-WorkflowSmokeSelection {
    param([Parameter(Mandatory)][string]$SmokeSelectionName)
    $catalog = Get-WorkflowCatalog
    if ($null -eq $catalog.smokeSelections -or @($catalog.smokeSelections).Count -eq 0) {
        throw 'Workflow catalog has no smoke selections.'
    }
    $selection = @($catalog.smokeSelections | Where-Object { [string]$_.name -ceq $SmokeSelectionName })
    if ($selection.Count -ne 1) { throw "Unknown or ambiguous workflow smoke selection: $SmokeSelectionName" }
    return Assert-WorkflowSmokeSelectionContract -Selection $selection[0]
}

function Assert-WorkflowSmokeSelectionContract {
    param([Parameter(Mandatory)]$Selection)
    $featureNames = @($selection.features.explicit | ForEach-Object { [string]$_ }) -join ','
    $expectedCaptureCount = if ([string]$selection.kind -ceq 'native') { 3 } else { 0 }
    $budgets = $selection.budgets
    $expectedPostBuildSeconds = if ([string]$selection.kind -ceq 'native') { 300 } else { 0 }
    $expectedHarnessArguments = @()
    if ([string]$selection.kind -ceq 'fixture-only') { $expectedHarnessArguments += '-FixturesOnly' }
    $expectedHarnessArguments += @('-Scenario', 'Phase2', '-Surface', 'Settings', '-CaptureTimeoutSeconds', '45',
        '-GracefulCloseSeconds', '6')
    if ([string]$selection.kind -ceq 'native') {
        $expectedHarnessArguments += @('-PostBuildTimeoutSeconds', '300', '-PostBuildShutdownReserveSeconds', '30')
    }
    $expectedHarnessArguments += @('-OverallTimeoutSeconds', '3600')
    $configuredHarnessArguments = @($selection.harnessArguments | ForEach-Object { [string]$_ })
    if ([string]$selection.scenario -cne 'Phase2' -or [string]$selection.surface -cne 'Settings' -or
        [string]$selection.kind -notin @('fixture-only', 'native') -or
        $featureNames -cne 'dev-smoke-binaries,qa-framebuffer-capture' -or
        [bool]$selection.features.defaultFeatures -ne $true -or
        [int]$selection.captureCount -ne $expectedCaptureCount -or
        [int]$budgets.overallSeconds -ne 3600 -or [int]$budgets.provenanceSeconds -ne 60 -or
        [int]$budgets.testPreparationSeconds -ne 1800 -or [int]$budgets.fixtureTestsSeconds -ne 600 -or
        [int]$budgets.fixtureCommandSeconds -ne 120 -or [int]$budgets.appBuildSeconds -ne 1800 -or
        [int]$budgets.postBuildNativeSeconds -ne $expectedPostBuildSeconds -or
        [int]$budgets.captureTimeoutSeconds -ne 45 -or [int]$budgets.gracefulCloseSeconds -ne 6 -or
        [int]$budgets.nativeShutdownReserveSeconds -ne 30 -or
        (($configuredHarnessArguments -join "`n") -cne ($expectedHarnessArguments -join "`n"))) {
        throw "Workflow smoke selection '$($selection.name)' does not match the supported Phase 2 Settings contract."
    }
    return $Selection
}

function New-WorkflowSmokeCommand {
    param([Parameter(Mandatory)]$Selection, [Parameter(Mandatory)][string]$ReceiptPath)
    $Selection = Assert-WorkflowSmokeSelectionContract -Selection $Selection
    if (!(Test-Path -LiteralPath $script:WorkflowJumpHarnessPath -PathType Leaf)) {
        throw 'The existing Jump Mode smoke harness is missing.'
    }
    $arguments = @('-NoLogo', '-NoProfile', '-NonInteractive', '-File', [IO.Path]::GetFullPath($script:WorkflowJumpHarnessPath))
    if ([string]$Selection.kind -ceq 'fixture-only') { $arguments += '-FixturesOnly' }
    $arguments += @('-Scenario', [string]$Selection.scenario, '-Surface', [string]$Selection.surface,
        '-CaptureTimeoutSeconds', [string]$Selection.budgets.captureTimeoutSeconds,
        '-GracefulCloseSeconds', [string]$Selection.budgets.gracefulCloseSeconds)
    if ([string]$Selection.kind -ceq 'native') {
        $arguments += @('-PostBuildTimeoutSeconds', [string]$Selection.budgets.postBuildNativeSeconds,
            '-PostBuildShutdownReserveSeconds', [string]$Selection.budgets.nativeShutdownReserveSeconds)
    }
    $arguments += @('-OverallTimeoutSeconds', [string]$Selection.budgets.overallSeconds, '-WorkflowReceiptPath', [IO.Path]::GetFullPath($ReceiptPath))
    return [pscustomobject][ordered]@{
        Executable = Get-WorkflowPowerShellPath
        ArgumentList = $arguments
        WorkingDirectory = $script:WorkflowRoot
        Started = $false
    }
}

function New-WorkflowSmokeRequest {
    param(
        [Parameter(Mandatory)][ValidateSet('Preview', 'Run')][string]$Action,
        [Parameter(Mandatory)][string]$SmokeSelectionName
    )
    $selection = Get-WorkflowSmokeSelection -SmokeSelectionName $SmokeSelectionName
    $catalog = Get-WorkflowCatalog
    $previewReceiptPath = Join-Path ([IO.Path]::GetTempPath()) 'MultiLauncher\workflow-evidence-preview\jump-run.json'
    $command = New-WorkflowSmokeCommand -Selection $selection -ReceiptPath $previewReceiptPath
    return [pscustomobject][ordered]@{
        SchemaVersion = 1
        Action = $Action
        WorkflowKind = 'jump-smoke'
        Mode = 'phase2-settings-smoke'
        ModeKind = 'smoke'
        Package = [string]$catalog.package
        TargetKind = 'binary'
        TargetName = 'multi_launcher'
        Features = [pscustomobject][ordered]@{
            DefaultFeatures = [bool]$selection.features.defaultFeatures
            Explicit = @($selection.features.explicit)
        }
        CompilerProfile = 'fast-dev'
        RunnerProfile = 'default'
        WorkKind = 'unknown'
        CacheState = 'unknown'
        BaselineSha = $null
        ControlledChangeId = $null
        ControlledChangePaths = @()
        BudgetSeconds = $selection.budgets
        SelectionName = [string]$selection.name
        SmokeSelectionName = [string]$selection.name
        SmokeSelection = $selection
        Scenario = [string]$selection.scenario
        Surface = [string]$selection.surface
        EvidenceKind = [string]$selection.evidenceKind
        CaptureCount = [int]$selection.captureCount
        NativePrerequisites = @($selection.nativePrerequisites)
        CatalogSha256 = Get-WorkflowCatalogSha256
        SourceFingerprintSha256 = Get-WorkflowSourceFingerprint
        CargoExecutable = $null
        NextestTool = $null
        Phases = @([pscustomobject][ordered]@{
            Name = 'jump-smoke'
            Stage = 'native'
            TimeoutSeconds = [int]$selection.budgets.overallSeconds
            Command = $command
        })
        Status = 'RESOLVED'
        ResolutionError = $null
        JumpReceiptPath = $null
    }
}

function Get-WorkflowCommand {
    param(
        [Parameter(Mandatory)][string]$Preset,
        [string]$WorkflowPhase,
        [string]$TargetKind,
        [string]$TestTarget,
        [string]$FilterExpression,
        [string]$BinariesMetadataPath,
        [string]$RunnerProfile,
        [string]$ReplayRecordingPath,
        [string[]]$TestArguments = @()
    )

    $wrapperArguments = @{ Preset = $Preset; PrintCommand = $true }
    if ($WorkflowPhase) { $wrapperArguments.WorkflowPhase = $WorkflowPhase }
    if ($TargetKind) { $wrapperArguments.TargetKind = $TargetKind }
    if ($TestTarget) { $wrapperArguments.TestTarget = $TestTarget }
    if ($FilterExpression) { $wrapperArguments.FilterExpression = $FilterExpression }
    if ($BinariesMetadataPath) { $wrapperArguments.BinariesMetadataPath = $BinariesMetadataPath }
    if ($RunnerProfile) { $wrapperArguments.RunnerProfile = $RunnerProfile }
    if ($ReplayRecordingPath) { $wrapperArguments.ReplayRecordingPath = $ReplayRecordingPath }
    if ($TestArguments.Count -gt 0) { $wrapperArguments.TestArguments = @($TestArguments) }

    $json = & $script:WorkflowCargoPath @wrapperArguments
    $record = $json | ConvertFrom-Json -ErrorAction Stop
    if ($record.Started -ne $false -or [string]::IsNullOrWhiteSpace([string]$record.Executable) -or $null -eq $record.ArgumentList) {
        throw "Cargo command resolver returned an invalid or executing command for preset '$Preset'."
    }
    return [pscustomobject][ordered]@{
        Executable = [IO.Path]::GetFullPath([string]$record.Executable)
        ArgumentList = @($record.ArgumentList | ForEach-Object { [string]$_ })
        WorkingDirectory = [IO.Path]::GetFullPath([string]$record.WorkingDirectory)
        Started = $false
    }
}

function Get-WorkflowFilterExpression {
    param([Parameter(Mandatory)][string]$ExactCase)
    if ([string]::IsNullOrWhiteSpace($ExactCase) -or $ExactCase.Contains("`n") -or $ExactCase.Contains("`r")) {
        throw 'Catalog exact-case filters must be non-empty single-line test names.'
    }
    $escaped = [regex]::Escape($ExactCase).Replace('/', '\/')
    return "test(/^$escaped`$/)"
}

function Get-WorkflowSelection {
    param([string]$SelectionName, [string]$TargetKind)
    $catalog = Get-WorkflowCatalog
    if (![string]::IsNullOrWhiteSpace($SelectionName)) {
        $selection = @($catalog.selections | Where-Object { [string]$_.name -ceq $SelectionName })
        if ($selection.Count -ne 1) { throw "Unknown or ambiguous workflow selection: $SelectionName" }
        if ($TargetKind -and $selection[0].targetKind -cne $TargetKind) {
            throw "Selection '$SelectionName' targets '$($selection[0].targetKind)', not '$TargetKind'."
        }
        return @($selection[0])
    }
    if ([string]::IsNullOrWhiteSpace($TargetKind)) { throw 'A selection name or target kind is required.' }
    return @($catalog.selections | Where-Object { [string]$_.targetKind -ceq $TargetKind })
}

function New-WorkflowPhaseTemplate {
    param(
        [Parameter(Mandatory)][string]$Preset,
        [Parameter(Mandatory)][string]$Phase,
        [Parameter(Mandatory)]$Selection,
        [string]$BinariesMetadataPath,
        [string]$FilterExpression,
        [string]$RunnerProfile,
        [string]$ReplayRecordingPath,
        [string[]]$TestArguments = @()
    )
    $testTarget = if ($Selection.targetKind -eq 'integration') { [string]$Selection.targetName } else { $null }
    return Get-WorkflowCommand -Preset $Preset -WorkflowPhase $Phase -TargetKind ([string]$Selection.targetKind) `
        -TestTarget $testTarget -FilterExpression $FilterExpression -BinariesMetadataPath $BinariesMetadataPath `
        -RunnerProfile $RunnerProfile -ReplayRecordingPath $ReplayRecordingPath -TestArguments $TestArguments
}

function New-WorkflowRequest {
    param(
        [Parameter(Mandatory)][ValidateSet('Preview', 'Discover', 'Run', 'Replay')][string]$Action,
        [Parameter(Mandatory)][string]$Mode,
        [string]$SelectionName,
        [ValidateSet('lib', 'integration')][string]$TargetKind,
        [string]$DiscoveryReceipt,
        [string]$RunnerProfile,
        [ValidateSet('unknown', 'no-op', 'edited-source', 'preparation', 'cold')][string]$WorkKind = 'unknown',
        [ValidateSet('unknown', 'warmed', 'clean', 'branch-switched', 'new-profile')][string]$CacheState = 'unknown',
        [string]$BaselineSha,
        [string]$ControlledChangeId,
        [string[]]$ControlledChangePaths = @(),
        [string[]]$TestArguments = @()
    )

    $catalog = Get-WorkflowCatalog
    $modeRecord = @($catalog.modes | Where-Object { [string]$_.name -ceq $Mode })
    if ($modeRecord.Count -ne 1) { throw "Unknown or ambiguous workflow mode: $Mode" }
    $modeRecord = $modeRecord[0]
    if ($null -eq $TestArguments) { $TestArguments = @() }
    foreach ($argument in $TestArguments) { if ($null -eq $argument) { throw 'TestArguments cannot contain null elements.' } }
    if ($modeRecord.kind -eq 'tests' -and $Action -in @('Preview', 'Run', 'Replay')) {
        $captureDisablingArgument = @($TestArguments | Where-Object {
            [string]::Equals([string]$_, '--nocapture', [StringComparison]::OrdinalIgnoreCase) -or
            [string]::Equals([string]$_, '--no-capture', [StringComparison]::OrdinalIgnoreCase)
        })
        if ($captureDisablingArgument.Count -gt 0) {
            throw 'Structured test workflows require captured test output; --nocapture and --no-capture are unsupported.'
        }
    }
    if ($RunnerProfile -and $modeRecord.kind -ne 'tests') { throw '-RunnerProfile is valid only for fast-dev-tests.' }
    if ($RunnerProfile -and $RunnerProfile -notmatch '^[A-Za-z0-9_][A-Za-z0-9_.-]*$') { throw 'Runner profile name is malformed.' }

    $request = [pscustomobject][ordered]@{
        SchemaVersion = 1
        Action = $Action
        Mode = [string]$modeRecord.name
        ModeKind = [string]$modeRecord.kind
        Package = [string]$catalog.package
        TargetKind = $null
        TargetName = $null
        Features = [pscustomobject][ordered]@{ DefaultFeatures = [bool]$catalog.features.defaultFeatures; Explicit = @($catalog.features.explicit) }
        CompilerProfile = [string]$modeRecord.compilerProfile
        RunnerProfile = if ($RunnerProfile) { $RunnerProfile } elseif ($modeRecord.runnerProfile) { [string]$modeRecord.runnerProfile } else { 'default' }
        BudgetSeconds = $modeRecord.budgets
        SelectionName = $null
        ExactCase = $null
        FilterExpression = $null
        SelectedIdentity = $null
        BinariesMetadataPath = $null
        DiscoveryReceipt = $null
        CatalogSha256 = Get-WorkflowCatalogSha256
        SourceFingerprintSha256 = Get-WorkflowSourceFingerprint
        WorkKind = $WorkKind
        CacheState = $CacheState
        BaselineSha = $BaselineSha
        ControlledChangeId = $ControlledChangeId
        ControlledChangePaths = @($ControlledChangePaths)
        CargoExecutable = $null
        NextestTool = $null
        Phases = @()
        Status = 'RESOLVED'
        ResolutionError = $null
    }

    if ($WorkKind -eq 'edited-source' -and ([string]::IsNullOrWhiteSpace($ControlledChangeId) -or $ControlledChangePaths.Count -eq 0)) {
        throw 'Edited-source workflows require -ControlledChangeId and at least one -ControlledChangePaths entry.'
    }
    if ($ControlledChangePaths -contains '' -or @($ControlledChangePaths | Where-Object { [string]::IsNullOrWhiteSpace([string]$_) }).Count -gt 0) {
        throw 'Controlled change paths cannot be empty.'
    }

    if ($Action -eq 'Discover' -and $modeRecord.kind -ne 'tests') { throw 'Discover requires the fast-dev-tests mode.' }
    if ($Action -eq 'Discover') {
        if ($SelectionName) { throw 'Discover accepts a target kind and discovers its catalog selections; it does not accept -SelectionName.' }
        if (!$TargetKind) { throw 'Discover requires -TargetKind lib or integration.' }
        if ($DiscoveryReceipt) { throw '-DiscoveryReceipt is not valid with Discover.' }
        if ($TestArguments.Count -gt 0) { throw 'Discover does not accept test-binary arguments.' }
        $selections = @(Get-WorkflowSelection -TargetKind $TargetKind)
        if ($selections.Count -eq 0) { throw "The catalog has no selections for target kind '$TargetKind'." }
        $request.TargetKind = $TargetKind
        $request.TargetName = if ($TargetKind -eq 'lib') { 'multi_launcher' } else { 'pure' }
        $request.CargoExecutable = Get-WorkflowCargoPath
        $request.NextestTool = Get-WorkflowNextestIdentity
        $request | Add-Member -NotePropertyName Selections -NotePropertyValue @($selections) -Force
        $request | Add-Member -NotePropertyName PrepareCommand -NotePropertyValue (New-WorkflowPhaseTemplate -Preset $modeRecord.cargoPreset -Phase 'prepare' -Selection $selections[0]) -Force
        $placeholder = Join-Path ([IO.Path]::GetTempPath()) 'multi-launcher-binaries-metadata-pending.json'
        $inventories = foreach ($selection in $selections) {
            $filter = Get-WorkflowFilterExpression -ExactCase ([string]$selection.exactCase)
            [pscustomobject][ordered]@{
                SelectionName = [string]$selection.name
                FilterExpression = $filter
                Command = New-WorkflowPhaseTemplate -Preset $modeRecord.cargoPreset -Phase 'inventory' -Selection $selection `
                    -BinariesMetadataPath $placeholder -FilterExpression $filter
            }
        }
        $request | Add-Member -NotePropertyName InventoryTemplates -NotePropertyValue @($inventories) -Force
        $request.Phases = @([pscustomobject][ordered]@{ Name = 'target-prepare'; Stage = 'compile'; TimeoutSeconds = [int]$modeRecord.budgets.preparationSeconds; Command = $request.PrepareCommand })
        return $request
    }

    if ($modeRecord.kind -eq 'build') {
        if ($SelectionName -or $TargetKind -or $DiscoveryReceipt -or $TestArguments.Count -gt 0) {
            throw "Build mode '$Mode' does not accept a test selection, discovery receipt, target kind, or test arguments."
        }
        $request.CargoExecutable = Get-WorkflowCargoPath
        $command = Get-WorkflowCommand -Preset ([string]$modeRecord.cargoPreset)
        $request.TargetKind = if ($modeRecord.cargoPreset -eq 'quick-check') { 'lib' } else { 'binary' }
        $request.TargetName = 'multi_launcher'
        $request.Phases = @([pscustomobject][ordered]@{
            Name = 'build'
            Stage = 'compile'
            TimeoutSeconds = [int]$modeRecord.budgets.executionSeconds
            Command = $command
        })
        if ($Action -eq 'Discover') { throw 'Build modes cannot discover test selections.' }
        return $request
    }

    if ($modeRecord.name -cne 'fast-dev-tests') { throw "Unsupported test mode: $Mode" }
    if ($Action -eq 'Discover') { throw 'Internal discovery resolution was not completed.' }
    if (!$SelectionName -or $TargetKind) { throw 'Preview and Run for test mode require exactly one -SelectionName.' }
    if ($TestArguments.Count -gt 0 -and $Action -notin @('Run', 'Replay', 'Preview')) { throw 'Test arguments are valid only for test Preview, Run, or Replay.' }
    $selection = @(Get-WorkflowSelection -SelectionName $SelectionName)
    if ($selection.Count -ne 1) { throw "Selection '$SelectionName' did not resolve to exactly one catalog entry." }
    $selection = $selection[0]
    $request.TargetKind = [string]$selection.targetKind
    $request.TargetName = [string]$selection.targetName
    $request.SelectionName = [string]$selection.name
    $request.ExactCase = [string]$selection.exactCase
    $request.FilterExpression = Get-WorkflowFilterExpression -ExactCase $request.ExactCase
    $request.CargoExecutable = Get-WorkflowCargoPath
    $request.NextestTool = Get-WorkflowNextestIdentity
    $request | Add-Member -NotePropertyName Selection -NotePropertyValue $selection -Force
    $request | Add-Member -NotePropertyName TestArguments -NotePropertyValue @($TestArguments) -Force
    $request | Add-Member -NotePropertyName ReplayRecordingPath -NotePropertyValue $null -Force
    $placeholder = Join-Path ([IO.Path]::GetTempPath()) 'multi-launcher-binaries-metadata-pending.json'
    $request | Add-Member -NotePropertyName PrepareCommand -NotePropertyValue (New-WorkflowPhaseTemplate -Preset $modeRecord.cargoPreset -Phase 'prepare' -Selection $selection) -Force
    $request | Add-Member -NotePropertyName InventoryCommand -NotePropertyValue (New-WorkflowPhaseTemplate -Preset $modeRecord.cargoPreset -Phase 'inventory' -Selection $selection `
        -BinariesMetadataPath $placeholder -FilterExpression $request.FilterExpression) -Force
    $request | Add-Member -NotePropertyName RunCommand -NotePropertyValue (New-WorkflowPhaseTemplate -Preset $modeRecord.cargoPreset -Phase 'run' -Selection $selection `
        -BinariesMetadataPath $placeholder -FilterExpression $request.FilterExpression -RunnerProfile $RunnerProfile -TestArguments $TestArguments) -Force
    $request.Phases = @(
        [pscustomobject][ordered]@{ Name = 'nextest-version'; Stage = 'provenance'; TimeoutSeconds = 20; Command = [pscustomobject][ordered]@{ Executable = $request.CargoExecutable; ArgumentList = @('nextest', '--version'); WorkingDirectory = $script:WorkflowRoot; Started = $false } },
        [pscustomobject][ordered]@{ Name = 'target-prepare'; Stage = 'compile'; TimeoutSeconds = [int]$modeRecord.budgets.preparationSeconds; Command = $request.PrepareCommand },
        [pscustomobject][ordered]@{ Name = 'selected-inventory'; Stage = 'provenance'; TimeoutSeconds = [int]$modeRecord.budgets.inventorySeconds; Command = $request.InventoryCommand },
        [pscustomobject][ordered]@{ Name = 'selected-run'; Stage = 'test'; TimeoutSeconds = [int]$modeRecord.budgets.executionSeconds; Command = $request.RunCommand }
    )

    if ($Action -eq 'Preview') {
        $validation = Test-WorkflowDiscoveryReceipt -Path $DiscoveryReceipt -Request $request
        if ($validation.Status -ne 'PASS') {
            $request.Status = $validation.Status
            $request.ResolutionError = $validation.Error
            return $request
        }
        Set-WorkflowRequestMetadataPath -Request $request -Path $validation.BinariesMetadataPath
        $request.SelectedIdentity = $validation.SelectedIdentity
        $request.DiscoveryReceipt = [IO.Path]::GetFullPath($DiscoveryReceipt)
    }
    elseif ($DiscoveryReceipt) { throw '-DiscoveryReceipt is used only by test Preview; Run always creates fresh target metadata.' }
    return $request
}

function New-WorkflowRequestFromCli {
    param(
        [Parameter(Mandatory)][ValidateSet('Preview', 'Discover', 'Run', 'Replay')][string]$Action,
        [string]$Mode,
        [string]$SelectionName,
        [string]$SmokeSelectionName,
        [string]$TargetKind,
        [string]$DiscoveryReceipt,
        [string]$ReplayReceipt,
        [string]$RunnerProfile,
        [ValidateSet('unknown', 'no-op', 'edited-source', 'preparation', 'cold')][string]$WorkKind = 'unknown',
        [ValidateSet('unknown', 'warmed', 'clean', 'branch-switched', 'new-profile')][string]$CacheState = 'unknown',
        [string]$BaselineSha,
        [string]$ControlledChangeId,
        [string[]]$ControlledChangePaths = @(),
        [string[]]$TestArguments = @()
    )

    if (![string]::IsNullOrWhiteSpace($SmokeSelectionName)) {
        if ($Action -notin @('Preview', 'Run') -or $Mode -or $SelectionName -or $TargetKind -or $DiscoveryReceipt -or $ReplayReceipt -or
            $RunnerProfile -or $WorkKind -ne 'unknown' -or $CacheState -ne 'unknown' -or $BaselineSha -or $ControlledChangeId -or
            $ControlledChangePaths.Count -gt 0 -or $TestArguments.Count -gt 0) {
            throw 'Smoke selections accept only -Action Preview or Run and -SmokeSelectionName.'
        }
        return New-WorkflowSmokeRequest -Action $Action -SmokeSelectionName $SmokeSelectionName
    }
    if ($Action -eq 'Replay') {
        if ([string]::IsNullOrWhiteSpace($ReplayReceipt)) { throw 'Replay requires -ReplayReceipt pointing to an original failed workflow receipt.' }
        if ($SelectionName -or $TargetKind -or $DiscoveryReceipt -or $RunnerProfile -or $WorkKind -ne 'unknown' -or $CacheState -ne 'unknown' -or $BaselineSha -or
            $ControlledChangeId -or $ControlledChangePaths.Count -gt 0 -or $TestArguments.Count -gt 0) {
            throw 'Replay takes its mode, selection, runner profile, and test arguments from the original receipt; only -ReplayReceipt is accepted.'
        }
        return New-WorkflowReplayRequest -Path $ReplayReceipt
    }
    if ([string]::IsNullOrWhiteSpace($Mode)) { throw '-Mode is required except for Replay, which reads it from -ReplayReceipt.' }
    if ($ReplayReceipt) { throw '-ReplayReceipt is valid only with -Action Replay.' }
    $requestArguments = @{ Action = $Action; Mode = $Mode }
    if (![string]::IsNullOrWhiteSpace($SelectionName)) { $requestArguments.SelectionName = $SelectionName }
    if (![string]::IsNullOrWhiteSpace($TargetKind)) { $requestArguments.TargetKind = $TargetKind }
    if (![string]::IsNullOrWhiteSpace($DiscoveryReceipt)) { $requestArguments.DiscoveryReceipt = $DiscoveryReceipt }
    if (![string]::IsNullOrWhiteSpace($RunnerProfile)) { $requestArguments.RunnerProfile = $RunnerProfile }
    if ($WorkKind -ne 'unknown') { $requestArguments.WorkKind = $WorkKind }
    if ($CacheState -ne 'unknown') { $requestArguments.CacheState = $CacheState }
    if (![string]::IsNullOrWhiteSpace($BaselineSha)) { $requestArguments.BaselineSha = $BaselineSha }
    if (![string]::IsNullOrWhiteSpace($ControlledChangeId)) { $requestArguments.ControlledChangeId = $ControlledChangeId }
    if ($ControlledChangePaths.Count -gt 0) { $requestArguments.ControlledChangePaths = @($ControlledChangePaths) }
    if ($PSBoundParameters.ContainsKey('TestArguments')) { $requestArguments.TestArguments = @($TestArguments) }
    return New-WorkflowRequest @requestArguments
}

function New-WorkflowToolReceipt {
    param([Parameter(Mandatory)]$Tool)
    return [ordered]@{
        path = [string]$Tool.Path
        sha256 = [string]$Tool.Sha256
    }
}

function Set-WorkflowRequestMetadataPath {
    param([Parameter(Mandatory)]$Request, [Parameter(Mandatory)][string]$Path)
    $metadataPath = [IO.Path]::GetFullPath($Path)
    $selection = $Request.Selection
    $mode = Get-WorkflowCatalog | ForEach-Object { @($_.modes | Where-Object { $_.name -ceq $Request.Mode })[0] }
    $filter = [string]$Request.FilterExpression
    $runnerProfile = if ($Request.RunnerProfile -and $Request.RunnerProfile -cne 'default') { [string]$Request.RunnerProfile } else { $null }
    $Request.InventoryCommand = New-WorkflowPhaseTemplate -Preset ([string]$mode.cargoPreset) -Phase 'inventory' -Selection $selection `
        -BinariesMetadataPath $metadataPath -FilterExpression $filter
    $Request.RunCommand = New-WorkflowPhaseTemplate -Preset ([string]$mode.cargoPreset) -Phase 'run' -Selection $selection `
        -BinariesMetadataPath $metadataPath -FilterExpression $filter -RunnerProfile $runnerProfile `
        -ReplayRecordingPath ([string]$Request.ReplayRecordingPath) -TestArguments @($Request.TestArguments)
    $Request.BinariesMetadataPath = $metadataPath
    $Request.Phases[2].Command = $Request.InventoryCommand
    $Request.Phases[3].Command = $Request.RunCommand
}

function New-WorkflowReplayRequest {
    param([Parameter(Mandatory)][string]$Path)
    $fullReceiptPath = [IO.Path]::GetFullPath($Path)
    $validated = Test-WorkflowReplayReceipt -Path $fullReceiptPath
    $original = $validated.Receipt
    $request = New-WorkflowRequest -Action Replay -Mode ([string]$original.Request.Mode) `
        -SelectionName ([string]$original.Request.SelectionName) -RunnerProfile ([string]$original.Request.RunnerProfile) `
        -TestArguments @($original.Request.TestArguments)
    if ($request.Status -ne 'RESOLVED') { throw "Original replay request could not be resolved: $($request.ResolutionError)" }
    if ($request.Package -cne [string]$original.Request.Package -or $request.TargetKind -cne [string]$original.Request.TargetKind -or
        $request.TargetName -cne [string]$original.Request.TargetName -or $request.CompilerProfile -cne [string]$original.Request.CompilerProfile -or
        $request.RunnerProfile -cne [string]$original.Request.RunnerProfile -or $request.Features.DefaultFeatures -ne [bool]$original.Request.Features.DefaultFeatures -or
        (@($request.Features.Explicit) -join "`n") -cne (@($original.Request.Features.Explicit) -join "`n") -or
        (@($request.TestArguments) -join "`n") -cne (@($original.Request.TestArguments) -join "`n")) {
        throw 'Original request targets, features, profiles, or forwarded test arguments no longer match the current resolver.'
    }
    $request.FilterExpression = '(' + (Get-WorkflowFilterExpression -ExactCase ([string]$request.ExactCase)) + ') & (' +
        (Get-WorkflowReplayFilterExpression -Failures $validated.Failures) + ')'
    $request.ReplayRecordingPath = $validated.ArchivePath
    $request | Add-Member -NotePropertyName ReplaySource -NotePropertyValue ([pscustomobject][ordered]@{
        ReceiptPath = $fullReceiptPath
        ReceiptSha256 = (Get-FileHash -LiteralPath $fullReceiptPath -Algorithm SHA256 -ErrorAction Stop).Hash.ToLowerInvariant()
        RecordingPath = $validated.ArchivePath
        RecordingSha256 = $validated.ArchiveSha256
        FailedIdentities = @($validated.Failures)
        SourceChanged = [bool]$validated.SourceChanged
        OriginalSourceFingerprintSha256 = $validated.OriginalSourceFingerprintSha256
        CurrentSourceFingerprintSha256 = $validated.CurrentSourceFingerprintSha256
    }) -Force
    $placeholder = Join-Path ([IO.Path]::GetTempPath()) 'multi-launcher-binaries-metadata-pending.json'
    $mode = Get-WorkflowCatalog | ForEach-Object { @($_.modes | Where-Object { $_.name -ceq $request.Mode })[0] }
    $runnerProfile = if ($request.RunnerProfile -cne 'default') { [string]$request.RunnerProfile } else { $null }
    $request.PrepareCommand = New-WorkflowPhaseTemplate -Preset ([string]$mode.cargoPreset) -Phase 'prepare' -Selection $request.Selection
    $request.InventoryCommand = New-WorkflowPhaseTemplate -Preset ([string]$mode.cargoPreset) -Phase 'inventory' -Selection $request.Selection `
        -BinariesMetadataPath $placeholder -FilterExpression (Get-WorkflowFilterExpression -ExactCase $request.ExactCase)
    $request.RunCommand = New-WorkflowPhaseTemplate -Preset ([string]$mode.cargoPreset) -Phase 'run' -Selection $request.Selection `
        -BinariesMetadataPath $placeholder -FilterExpression $request.FilterExpression -RunnerProfile $runnerProfile `
        -ReplayRecordingPath $request.ReplayRecordingPath -TestArguments @($request.TestArguments)
    $request.Phases[1].Command = $request.PrepareCommand
    $request.Phases[2].Command = $request.InventoryCommand
    $request.Phases[3].Command = $request.RunCommand
    $request | Add-Member -NotePropertyName ReplayReceipt -NotePropertyValue $fullReceiptPath -Force
    return $request
}

function Test-WorkflowDiscoveryReceipt {
    param([string]$Path, [Parameter(Mandatory)]$Request)
    if ([string]::IsNullOrWhiteSpace($Path)) {
        return [pscustomobject]@{ Status = 'DISCOVERY_REQUIRED'; Error = 'A test preview requires -DiscoveryReceipt from a successful Discover action.' }
    }
    if (!(Test-Path -LiteralPath $Path -PathType Leaf)) {
        return [pscustomobject]@{ Status = 'DISCOVERY_REQUIRED'; Error = "Discovery receipt is missing: $Path" }
    }
    try { $receipt = Get-Content -LiteralPath $Path -Raw -ErrorAction Stop | ConvertFrom-Json -ErrorAction Stop }
    catch { return [pscustomobject]@{ Status = 'STALE_DISCOVERY'; Error = "Discovery receipt is malformed: $($_.Exception.Message)" } }

    if ($receipt.schemaVersion -ne 1 -or $receipt.status -cne 'PASS') {
        return [pscustomobject]@{ Status = 'STALE_DISCOVERY'; Error = 'Discovery receipt is not a successful supported version.' }
    }
    if ($receipt.catalogSha256 -cne $Request.CatalogSha256 -or $receipt.sourceFingerprintSha256 -cne $Request.SourceFingerprintSha256) {
        return [pscustomobject]@{ Status = 'STALE_DISCOVERY'; Error = 'Discovery receipt does not match the current catalog or source fingerprint; run Discover again.' }
    }
    if ($receipt.targetKind -cne $Request.TargetKind) {
        return [pscustomobject]@{ Status = 'STALE_DISCOVERY'; Error = 'Discovery receipt target kind does not match this selection.' }
    }
    if ($receipt.cargoExecutable -cne $Request.CargoExecutable -or $receipt.cargoExecutableSha256 -cne (Get-FileHash -LiteralPath $Request.CargoExecutable -Algorithm SHA256).Hash.ToLowerInvariant()) {
        return [pscustomobject]@{ Status = 'STALE_DISCOVERY'; Error = 'Cargo executable identity changed after discovery.' }
    }
    if ($receipt.nextestTool.path -cne $Request.NextestTool.Path -or $receipt.nextestTool.sha256 -cne $Request.NextestTool.Sha256) {
        return [pscustomobject]@{ Status = 'STALE_DISCOVERY'; Error = 'Cargo Nextest tool identity changed after discovery.' }
    }
    $metadataPath = [string]$receipt.binariesMetadataPath
    if ([string]::IsNullOrWhiteSpace($metadataPath) -or !(Test-Path -LiteralPath $metadataPath -PathType Leaf)) {
        return [pscustomobject]@{ Status = 'STALE_DISCOVERY'; Error = 'Prepared binary metadata is missing; run Discover again.' }
    }
    $metadataHash = (Get-FileHash -LiteralPath $metadataPath -Algorithm SHA256 -ErrorAction Stop).Hash.ToLowerInvariant()
    if ($metadataHash -cne [string]$receipt.binariesMetadataSha256) {
        return [pscustomobject]@{ Status = 'STALE_DISCOVERY'; Error = 'Prepared binary metadata changed after discovery.' }
    }
    $discovered = @($receipt.selections | Where-Object { [string]$_.name -ceq $Request.SelectionName })
    if ($discovered.Count -ne 1 -or $discovered[0].status -cne 'PASS' -or [int]$discovered[0].selectedCaseCount -ne 1) {
        return [pscustomobject]@{ Status = 'STALE_DISCOVERY'; Error = "Discovery receipt does not contain exactly one validated case for '$($Request.SelectionName)'." }
    }
    $identity = $discovered[0].selectedIdentity
    if ($null -eq $identity -or $identity.packageName -cne $Request.Package -or $identity.binaryName -cne $Request.TargetName -or
        $identity.kind -cne [string]$Request.Selection.nextestKind -or $identity.testName -cne $Request.ExactCase -or [string]::IsNullOrWhiteSpace([string]$identity.binaryId)) {
        return [pscustomobject]@{ Status = 'STALE_DISCOVERY'; Error = 'Discovered package, target, kind, binary, or exact case no longer matches the catalog.' }
    }
    return [pscustomobject]@{ Status = 'PASS'; Error = $null; BinariesMetadataPath = $metadataPath; SelectedIdentity = $identity }
}

function Test-WorkflowPathWithin {
    param([string]$Candidate, [string]$Directory)
    $candidatePath = [IO.Path]::GetFullPath($Candidate).TrimEnd([char[]]@('\', '/'))
    $directoryPath = [IO.Path]::GetFullPath($Directory).TrimEnd([char[]]@('\', '/'))
    if ([string]::IsNullOrEmpty($directoryPath)) { $directoryPath = [IO.Path]::GetPathRoot([IO.Path]::GetFullPath($Directory)) }
    $prefix = $directoryPath + [IO.Path]::DirectorySeparatorChar
    return $candidatePath.Equals($directoryPath, [StringComparison]::OrdinalIgnoreCase) -or
        $candidatePath.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase)
}

function New-WorkflowEvidenceDirectory {
    param([string]$EvidenceDirectory)
    $root = if ([string]::IsNullOrWhiteSpace($EvidenceDirectory)) { Join-Path ([IO.Path]::GetTempPath()) 'Multi_Launcher\workflows' } else { [IO.Path]::GetFullPath($EvidenceDirectory) }
    if (Test-WorkflowPathWithin -Candidate $root -Directory $script:WorkflowRoot) { throw "Workflow evidence must be outside the source worktree: $root" }
    [IO.Directory]::CreateDirectory($root) | Out-Null
    $runPath = Join-Path $root ('workflow_' + [DateTimeOffset]::UtcNow.ToString('yyyyMMddTHHmmssfffZ') + '_' + [guid]::NewGuid().ToString('N'))
    [IO.Directory]::CreateDirectory($runPath) | Out-Null
    return $runPath
}

function Get-WorkflowPhaseSummary {
    param([Parameter(Mandatory)][string]$Name, [Parameter(Mandatory)]$Command, [Parameter(Mandatory)]$Measurement)
    return [pscustomobject][ordered]@{
        Name = $Name
        Executable = [string]$Command.Executable
        ArgumentList = @($Command.ArgumentList)
        WorkingDirectory = [string]$Command.WorkingDirectory
        DeadlineUtc = $Measurement.DeadlineUtc
        TimeoutSeconds = $Measurement.TimeoutSeconds
        WallSeconds = $Measurement.WallSeconds
        TimedOut = [bool]$Measurement.TimedOut
        ExitCode = $Measurement.ExitCode
        Succeeded = [bool]$Measurement.Succeeded
        ExecutionReason = $Measurement.ExecutionReason
        ProcessStarted = [bool]$Measurement.ProcessStarted
        CleanupVerified = [bool]$Measurement.CleanupVerified
        CleanupErrors = @($Measurement.CleanupErrors)
        OutputDrainVerified = [bool]$Measurement.OutputDrainVerified
        OutputBytes = $Measurement.OutputBytes
        OwnedProcesses = @($Measurement.OwnedProcesses)
        PostCleanup = @($Measurement.PostCleanup)
        RecordingErrors = @($Measurement.RecordingErrors)
        AuxiliaryProcessPolicy = [string]$Measurement.AuxiliaryProcessPolicy
        AuxiliaryPreflight = $Measurement.AuxiliaryPreflight
        AuxiliaryProcessClassifications = @($Measurement.AuxiliaryProcessClassifications)
        ForcedJobRetirementAttempted = [bool]$Measurement.ForcedJobRetirementAttempted
        AuxiliaryDisposition = [string]$Measurement.AuxiliaryDisposition
        AuxiliaryLeaseCleanupVerified = [bool]$Measurement.AuxiliaryLeaseCleanupVerified
        AuxiliaryLeaseCleanupErrors = @($Measurement.AuxiliaryLeaseCleanupErrors)
        EvidenceDirectory = [string]$Measurement.EvidenceDirectory
        MeasurementPath = [string]$Measurement.ManifestPath
        StdoutPath = [string]$Measurement.StdoutPath
        StderrPath = [string]$Measurement.StderrPath
    }
}

function Invoke-WorkflowCommand {
    param(
        [Parameter(Mandatory)]$Request,
        [Parameter(Mandatory)][string]$EvidenceDirectory,
        [Parameter(Mandatory)][string]$Name,
        [Parameter(Mandatory)][string]$Stage,
        [Parameter(Mandatory)][int]$TimeoutSeconds,
        [Parameter(Mandatory)]$Command,
        [hashtable]$EnvironmentOverrides = @{},
        [scriptblock]$ProcessInvoker
    )
    if ($null -eq $ProcessInvoker) {
        $auxiliaryPolicy = 'none'
        if ($Stage -eq 'compile') {
            $catalogMode = @((Get-WorkflowCatalog).modes | Where-Object { [string]$_.name -ceq [string]$Request.Mode })
            $isFixedBuildPhase = $Request.Action -eq 'Run' -and $catalogMode.Count -eq 1 -and
                [string]$catalogMode[0].kind -ceq 'build' -and $Name -ceq 'build' -and
                $Request.Phases.Count -eq 1 -and [object]::ReferenceEquals($Command, $Request.Phases[0].Command)
            $isFixedTargetPrepare = $Request.Action -in @('Discover', 'Run', 'Replay') -and
                [string]$Request.Mode -ceq 'fast-dev-tests' -and $Name -ceq 'target-prepare' -and
                [object]::ReferenceEquals($Command, $Request.PrepareCommand)
            if ($isFixedBuildPhase -or $isFixedTargetPrepare) { $auxiliaryPolicy = 'verified-msvc-vctip' }
        }
        $measureArguments = @{
            Executable = [string]$Command.Executable
            ArgumentList = @($Command.ArgumentList)
            ScenarioId = ('workflow-{0}-{1}-{2}' -f $Request.Mode, $Name, [guid]::NewGuid().ToString('N'))
            EvidenceDirectory = $EvidenceDirectory
            WorkingDirectory = [string]$Command.WorkingDirectory
            SourceRoot = $script:WorkflowRoot
            Profile = [string]$Request.CompilerProfile
            WorkKind = [string]$Request.WorkKind
            CacheState = [string]$Request.CacheState
            BaselineSha = [string]$Request.BaselineSha
            ControlledChangeId = [string]$Request.ControlledChangeId
            ControlledChangePaths = @($Request.ControlledChangePaths)
            ComparisonSelection = @{
                Mode = [string]$Request.Mode
                ModeKind = [string]$Request.ModeKind
                Package = [string]$Request.Package
                TargetKind = [string]$Request.TargetKind
                TargetName = [string]$Request.TargetName
                Features = [pscustomobject]@{ DefaultFeatures = [bool]$Request.Features.DefaultFeatures; Explicit = @($Request.Features.Explicit) }
                CompilerProfile = [string]$Request.CompilerProfile
                RunnerProfile = [string]$Request.RunnerProfile
                SelectionName = [string]$Request.SelectionName
                ExactCase = [string]$Request.ExactCase
            }
            Stage = $Stage
            TimeoutSeconds = $TimeoutSeconds
            CleanupTimeoutSeconds = 5
            EnvironmentOverrides = $EnvironmentOverrides
            AuxiliaryProcessPolicy = $auxiliaryPolicy
        }
        $measurement = & $script:WorkflowMeasurePath @measureArguments
    }
    else {
        $measurement = & $ProcessInvoker @{
            Request = $Request
            Name = $Name
            Stage = $Stage
            TimeoutSeconds = $TimeoutSeconds
            Command = $Command
            EvidenceDirectory = $EvidenceDirectory
            EnvironmentOverrides = $EnvironmentOverrides
        }
    }
    if ($null -eq $measurement) { throw "Process owner returned no result for phase '$Name'." }
    return $measurement
}

function Get-WorkflowFailureExitCode {
    param($Measurement)
    if ($Measurement.TimedOut) { return 124 }
    if ($null -ne $Measurement.ExitCode -and [int]$Measurement.ExitCode -ne 0) { return [int]$Measurement.ExitCode }
    return 1
}

function Assert-WorkflowBinaryMetadata {
    param([Parameter(Mandatory)][string]$Path)
    $metadata = Get-Content -LiteralPath $Path -Raw -ErrorAction Stop | ConvertFrom-Json -AsHashtable -ErrorAction Stop
    $binaries = Get-TestPerfField $metadata 'rust-binaries'
    if ($null -eq $binaries -or @($binaries).Count -eq 0) { throw 'Nextest binary metadata is missing rust-binaries.' }
    return $metadata
}

function Get-WorkflowSelectedIdentity {
    param([Parameter(Mandatory)][string]$InventoryPath, [Parameter(Mandatory)]$Selection)
    $cases = @(Convert-TestPerfInventory -Path $InventoryPath)
    $selectedCount = [int](Get-TestPerfSelectedCaseCount -Cases $cases)
    $matches = @($cases | Where-Object { !$_.Ignored -and $_.FilterStatus -eq 'matches' })
    if ($selectedCount -ne 1 -or $matches.Count -ne 1) {
        throw "Exact selection '$($Selection.name)' matched $selectedCount runnable cases; exactly one is required."
    }
    $case = $matches[0]
    if ($case.TestName -cne [string]$Selection.exactCase -or $case.PackageName -cne [string]$Selection.package -or
        $case.BinaryName -cne [string]$Selection.targetName -or $case.Kind -cne [string]$Selection.nextestKind -or
        [string]::IsNullOrWhiteSpace([string]$case.BinaryId)) {
        throw "Nextest identity for '$($Selection.name)' does not match its package, target, kind, and exact-case catalog entry."
    }
    return [pscustomobject][ordered]@{
        BinaryId = [string]$case.BinaryId
        PackageName = [string]$case.PackageName
        BinaryName = [string]$case.BinaryName
        Kind = [string]$case.Kind
        TestName = [string]$case.TestName
    }
}

function Write-WorkflowJson {
    param([Parameter(Mandatory)][string]$Path, [Parameter(Mandatory)]$Value)
    $json = ConvertTo-Json -InputObject $Value -Depth 18 -ErrorAction Stop
    Set-Content -LiteralPath $Path -Value $json -Encoding utf8 -ErrorAction Stop
}

function Test-WorkflowSmokeMeasurementContract {
    param([Parameter(Mandatory)]$Summary, [Parameter(Mandatory)]$Measurement)
    $name = [string]$Summary.name
    $executable = [IO.Path]::GetFullPath([string]$Measurement.Executable)
    $arguments = @($Measurement.ArgumentList | ForEach-Object { [string]$_ })
    $cargo = [IO.Path]::GetFileName($executable) -ieq 'cargo.exe'
    $manifest = Join-Path $script:WorkflowRoot 'Cargo.toml'
    $features = 'dev-smoke-binaries,qa-framebuffer-capture'
    switch ($name) {
        'cargo-metadata' {
            $expected = @('metadata', '--no-deps', '--format-version', '1', '--manifest-path', $manifest)
            return $cargo -and [string]$Measurement.Stage -ceq 'provenance' -and
                (($arguments -join "`n") -ceq ($expected -join "`n")) -and
                [string]$Measurement.AuxiliaryProcessPolicy -ceq 'none'
        }
        'fixture-binary-prepare' {
            $expected = @('nextest', 'list', '--list-type', 'binaries-only', '--message-format', 'json', '--cargo-profile', 'fast-dev',
                '--features', $features, '--bin', 'jump_mode_fixture', '--manifest-path', $manifest)
            return $cargo -and [string]$Measurement.Stage -ceq 'compile' -and
                (($arguments -join "`n") -ceq ($expected -join "`n")) -and
                [string]$Measurement.AuxiliaryProcessPolicy -ceq 'verified-msvc-vctip'
        }
        'fixture-generator-build' {
            $expected = @('build', '--manifest-path', $manifest, '--profile', 'fast-dev', '--features', $features, '--bin', 'jump_mode_fixture')
            return $cargo -and [string]$Measurement.Stage -ceq 'compile' -and
                (($arguments -join "`n") -ceq ($expected -join "`n")) -and
                [string]$Measurement.AuxiliaryProcessPolicy -ceq 'verified-msvc-vctip'
        }
        'fixture-tests' {
            $binaryDirectory = [IO.Path]::GetDirectoryName($executable)
            return !$cargo -and [string]$Measurement.Stage -ceq 'test' -and
                [IO.Path]::GetFileName($executable) -match '^jump_mode_fixture-[A-Za-z0-9_-]+\.exe$' -and
                [IO.Path]::GetFileName($binaryDirectory) -ieq 'deps' -and
                (($arguments -join "`n") -ceq '--test-threads=1') -and
                [string]$Measurement.AuxiliaryProcessPolicy -ceq 'none'
        }
        { $_ -in @('fixture-generate', 'fixture-verify') } {
            $operation = if ($name -ceq 'fixture-generate') { 'generate' } else { 'verify' }
            $fixtureProfile = if ($arguments.Count -ge 2) { [IO.Path]::GetFullPath($arguments[1]) } else { '' }
            $harnessEvidenceRoot = Join-Path $script:WorkflowRoot 'dev\jump_mode\evidence'
            return !$cargo -and [IO.Path]::GetFileName($executable) -ieq 'jump_mode_fixture.exe' -and
                [string]$Measurement.Stage -ceq 'test' -and $arguments.Count -eq 3 -and $arguments[0] -ceq $operation -and
                $arguments[2] -ceq 'phase2-settings' -and [IO.Path]::GetFileName($fixtureProfile) -ceq 'profile' -and
                (Test-WorkflowPathWithin -Candidate $fixtureProfile -Directory $harnessEvidenceRoot) -and
                [string]$Measurement.AuxiliaryProcessPolicy -ceq 'none'
        }
        'app-build' {
            $expected = @('build', '--manifest-path', $manifest, '--profile', 'fast-dev', '--features', $features, '--bin', 'multi_launcher')
            return $cargo -and [string]$Measurement.Stage -ceq 'compile' -and
                (($arguments -join "`n") -ceq ($expected -join "`n")) -and
                [string]$Measurement.AuxiliaryProcessPolicy -ceq 'verified-msvc-vctip'
        }
        default { return $false }
    }
}

function Test-WorkflowSmokeReceipt {
    param([Parameter(Mandatory)][string]$Path, [Parameter(Mandatory)]$Request)

    $receipt = [pscustomobject][ordered]@{
        Valid = $false
        Classification = 'RECEIPT_MISMATCH'
        Error = $null
        Link = $null
        Manifest = $null
        Scenarios = @()
        Lifecycle = $null
        EvidenceRoot = $null
    }
    try {
        $fullReceiptPath = [IO.Path]::GetFullPath($Path)
        if (!(Test-Path -LiteralPath $fullReceiptPath -PathType Leaf)) { throw 'Jump Mode did not write the requested workflow link receipt.' }
        $link = Get-Content -LiteralPath $fullReceiptPath -Raw -ErrorAction Stop | ConvertFrom-Json -AsHashtable -ErrorAction Stop
        if ($link.schema_version -ne 1 -or [string]$link.selection_name -cne [string]$Request.SmokeSelectionName -or
            [string]$link.selection_kind -cne [string]$Request.SmokeSelection.kind -or
            [string]$link.scenario -cne [string]$Request.Scenario -or [string]$link.surface -cne [string]$Request.Surface) {
            throw 'Jump Mode link receipt does not match the selected smoke scenario, surface, and evidence kind.'
        }
        $runId = [string]$link.run_id
        if ([string]::IsNullOrWhiteSpace($runId) -or $runId -notmatch '^jump_m13_phase2_settings_[0-9]{8}T[0-9]{6}Z_[0-9a-f]{8}$') {
            throw 'Jump Mode link receipt has an invalid unique Phase 2 Settings run id.'
        }
        $evidenceRoot = [IO.Path]::GetFullPath([string]$link.evidence_root)
        $allowedEvidenceRoot = Join-Path $script:WorkflowRoot 'dev\jump_mode\evidence'
        if (!(Test-WorkflowPathWithin -Candidate $evidenceRoot -Directory $allowedEvidenceRoot) -or
            [IO.Path]::GetFileName($evidenceRoot) -cne $runId -or
            [IO.Path]::GetDirectoryName($evidenceRoot).TrimEnd('\') -ine [IO.Path]::GetFullPath($allowedEvidenceRoot).TrimEnd('\')) {
            throw 'Jump Mode link receipt does not point to its own unique run directory under the harness evidence root.'
        }
        $manifestPath = [IO.Path]::GetFullPath([string]$link.manifest_path)
        $scenariosPath = [IO.Path]::GetFullPath([string]$link.scenarios_path)
        $lifecyclePath = [IO.Path]::GetFullPath([string]$link.lifecycle_path)
        foreach ($candidate in @($manifestPath, $scenariosPath, $lifecyclePath)) {
            if (!(Test-WorkflowPathWithin -Candidate $candidate -Directory $evidenceRoot) -or
                !(Test-Path -LiteralPath $candidate -PathType Leaf)) {
                throw 'A linked Jump Mode manifest, scenario, or lifecycle receipt is missing or outside its run directory.'
            }
        }
        if ($manifestPath -ine (Join-Path $evidenceRoot 'manifest.json') -or
            $scenariosPath -ine (Join-Path $evidenceRoot 'scenarios.json') -or
            $lifecyclePath -ine (Join-Path $evidenceRoot 'process_lifecycle.json')) {
            throw 'Jump Mode link paths do not identify the canonical run receipts.'
        }
        $manifest = Get-Content -LiteralPath $manifestPath -Raw -ErrorAction Stop | ConvertFrom-Json -AsHashtable -ErrorAction Stop
        $scenarios = @(Get-Content -LiteralPath $scenariosPath -Raw -ErrorAction Stop | ConvertFrom-Json -AsHashtable -ErrorAction Stop)
        $lifecycle = Get-Content -LiteralPath $lifecyclePath -Raw -ErrorAction Stop | ConvertFrom-Json -AsHashtable -ErrorAction Stop
        if ([string]$manifest.run_id -cne $runId -or [string]$manifest.repository_root -ine $script:WorkflowRoot -or $scenarios.Count -ne 2 -or
            [string]$scenarios[0].scenario_id -cne 'm13-fixture-validation' -or [string]$scenarios[1].scenario_id -cne 'm13-phase2-settings') {
            throw 'Linked Jump Mode receipts do not agree on the run id, repository, or expected Phase 2 Settings scenarios.'
        }
        $fixtureStatus = [string]$scenarios[0].status
        $scenarioStatus = [string]$scenarios[1].status
        $receipt.Manifest = $manifest
        $receipt.Scenarios = $scenarios
        $receipt.Lifecycle = $lifecycle
        $receipt.Link = $link
        $receipt.EvidenceRoot = $evidenceRoot
        $receipt.Valid = $true

        $measurements = @($link.command_measurements)
        $measurementNames = @($measurements | ForEach-Object { [string]$_.name })
        $expectedMeasurementNames = @('cargo-metadata', 'fixture-binary-prepare', 'fixture-generator-build', 'fixture-tests', 'fixture-generate', 'fixture-verify')
        $mustHaveAllMeasurements = [string]$manifest.status -ceq 'PASS' -or [string]$manifest.status -ceq 'NOT_RUN_ENVIRONMENT'
        $nativeEnvironmentNotRun = [string]$link.selection_kind -ceq 'native' -and
            [string]$manifest.status -ceq 'NOT_RUN_ENVIRONMENT'
        if ([string]$link.selection_kind -ceq 'native' -and !$nativeEnvironmentNotRun) { $expectedMeasurementNames += 'app-build' }
        $expectedCount = if ($mustHaveAllMeasurements) { $expectedMeasurementNames.Count } else { $null }
        $measurementPrefixValid = $measurementNames.Count -le $expectedMeasurementNames.Count
        for ($index = 0; $index -lt $measurementNames.Count -and $measurementPrefixValid; $index++) {
            if ($measurementNames[$index] -cne $expectedMeasurementNames[$index]) { $measurementPrefixValid = $false }
        }
        if (!$measurementPrefixValid -or
            ($mustHaveAllMeasurements -and $measurementNames.Count -ne [int]$expectedCount) -or
            ($measurementNames.Count -eq 0 -and [string]$manifest.status -ceq 'PASS')) {
            $receipt.Classification = 'RECEIPT_MISMATCH'
            $receipt.Error = 'Linked Jump Mode measurements do not match the ordered commands required by this run outcome.'
            return $receipt
        }
        $measurementPaths = [System.Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
        $measurementRoot = Join-Path ([IO.Path]::GetDirectoryName($fullReceiptPath)) 'jump-measurements'
        $measurementCleanupUncertain = $false
        foreach ($measurementSummary in $measurements) {
            if ([string]::IsNullOrWhiteSpace([string]$measurementSummary.measurement_path)) {
                $receipt.Classification = 'RECEIPT_MISMATCH'
                $receipt.Error = 'A linked checked-command measurement path is empty.'
                return $receipt
            }
            $measurementPath = [IO.Path]::GetFullPath([string]$measurementSummary.measurement_path)
            if (!(Test-WorkflowPathWithin -Candidate $measurementPath -Directory $measurementRoot) -or
                [IO.Path]::GetFileName($measurementPath) -cne 'measurement.json') {
                $receipt.Classification = 'RECEIPT_MISMATCH'
                $receipt.Error = 'A linked checked-command measurement is outside the workflow-owned measurement directory.'
                return $receipt
            }
            if (!$measurementPaths.Add($measurementPath) -or !(Test-Path -LiteralPath $measurementPath -PathType Leaf) -or
                [string]$measurementSummary.status -notin @('PASS', 'FAIL') -or
                ($mustHaveAllMeasurements -and [string]$measurementSummary.status -cne 'PASS')) {
                $receipt.Classification = 'RECEIPT_MISMATCH'
                $receipt.Error = 'A nested checked-command measurement is missing, duplicated, or has an inconsistent status.'
                return $receipt
            }
            $measurement = Get-Content -LiteralPath $measurementPath -Raw -ErrorAction Stop | ConvertFrom-Json -AsHashtable -ErrorAction Stop
            if ([string]$measurement.SourceSha -notmatch '^(?:[0-9a-f]{40}|[0-9a-f]{64})$' -or
                [string]$measurementSummary.source_sha -cne [string]$measurement.SourceSha -or
                [string]$manifest.source_head -cne [string]$measurement.SourceSha) {
                $receipt.Classification = 'RECEIPT_MISMATCH'
                $receipt.Error = 'A linked checked-command measurement does not match the run source HEAD.'
                return $receipt
            }
            if (([string]$measurementSummary.status -ceq 'PASS') -ne [bool]$measurement.Succeeded) {
                $receipt.Classification = 'RECEIPT_MISMATCH'
                $receipt.Error = 'A linked checked-command measurement status does not agree with its owner receipt.'
                return $receipt
            }
            if (!$measurement.CleanupVerified -or !$measurement.OutputDrainVerified -or @($measurement.CleanupErrors).Count -gt 0 -or
                @($measurement.RecordingErrors).Count -gt 0) { $measurementCleanupUncertain = $true }
            $expectedStage = switch ([string]$measurementSummary.name) {
                'cargo-metadata' { 'provenance' }
                { $_ -in @('fixture-binary-prepare', 'fixture-generator-build', 'app-build') } { 'compile' }
                default { 'test' }
            }
            if ([string]$measurement.Stage -cne $expectedStage -or
                (($measurement.ArgumentList -join "`n") -cne (@($measurementSummary.arguments) -join "`n")) -or
                [string]$measurementSummary.stage -cne $expectedStage -or
                ![bool](Test-WorkflowSmokeMeasurementContract -Summary $measurementSummary -Measurement $measurement)) {
                $receipt.Classification = 'RECEIPT_MISMATCH'
                $receipt.Error = 'A nested command measurement does not match its approved target, arguments, stage, or compiler policy.'
                return $receipt
            }
        }

        $cleanupUncertain = $false
        if ($measurementCleanupUncertain) { $cleanupUncertain = $true }
        if ([string]$link.selection_kind -ceq 'native' -and $manifest.executable -and $manifest.executable.pid) {
            $cleanupUncertain = !$lifecycle.owned_tree_verified -or !$lifecycle.process_output_drain_verified -or
                [string]$lifecycle.post_cleanup_state -cne 'owned_tree_gone' -or @($lifecycle.cleanup_issues).Count -gt 0 -or $cleanupUncertain
        }
        if ($cleanupUncertain) {
            $receipt.Classification = 'CLEANUP_UNCERTAINTY'
            $receipt.Error = 'Native run lacks verified owned-process cleanup, process-log drain, or a clean lifecycle receipt.'
            return $receipt
        }

        if ([string]$link.selection_kind -ceq 'fixture-only') {
            if ([string]$manifest.status -ceq 'PASS' -and $fixtureStatus -ceq 'PASS' -and $scenarioStatus -ceq 'BLOCKED' -and
                [string]$manifest.phase -ceq 'complete' -and [int]$manifest.capture.count -eq 0 -and
                [string]$lifecycle.post_cleanup_state -ceq 'not_started' -and [string]$link.native_status -ceq 'NOT_RUN') {
                $receipt.Classification = 'FIXTURE_PASS'
                return $receipt
            }
            $receipt.Classification = 'SCENARIO_CAPTURE_FAILURE'
            $receipt.Error = 'Fixture-only receipt does not prove typed fixture success with native execution explicitly not run.'
            return $receipt
        }

        if ([string]$manifest.status -ceq 'NOT_RUN_ENVIRONMENT' -and $fixtureStatus -ceq 'PASS' -and
            $scenarioStatus -ceq 'NOT_RUN_ENVIRONMENT' -and [int]$link.capture_count -eq 0 -and
            [string]$link.native_status -ceq 'NOT_RUN_ENVIRONMENT') {
            $receipt.Classification = 'ENVIRONMENT_NATIVE_NOT_RUN'
            return $receipt
        }

        if ([string]$manifest.status -ceq 'PASS' -and $fixtureStatus -ceq 'PASS' -and $scenarioStatus -ceq 'PASS' -and
            [string]$manifest.phase -ceq 'phase2_complete' -and [string]$link.native_status -ceq 'PASS') {
            if ([int]$manifest.post_build_native_timeout_seconds -ne [int]$Request.BudgetSeconds.postBuildNativeSeconds -or
                [int]$manifest.post_build_native_shutdown_reserve_seconds -ne [int]$Request.BudgetSeconds.nativeShutdownReserveSeconds -or
                [bool]$manifest.post_build_native_deadline_reached -or [bool]$link.post_build_native_deadline_reached -or
                [int]$link.capture_count -ne [int]$Request.CaptureCount -or [int]$manifest.capture.count -ne [int]$Request.CaptureCount -or
                @($manifest.capture.captures).Count -ne [int]$Request.CaptureCount -or
                @($scenarios[1].captures).Count -ne [int]$Request.CaptureCount -or
                !$manifest.post_build_native_started_at_utc -or !$manifest.post_build_native_deadline_utc -or
                !$manifest.post_build_native_finished_at_utc) {
                $receipt.Classification = 'RECEIPT_MISMATCH'
                $receipt.Error = 'Native receipt does not record the exact capture count and 300-second post-build deadline.'
                return $receipt
            }
            $nativeStarted = [DateTimeOffset]::Parse([string]$manifest.post_build_native_started_at_utc).UtcDateTime
            $nativeDeadline = [DateTimeOffset]::Parse([string]$manifest.post_build_native_deadline_utc).UtcDateTime
            $nativeFinished = [DateTimeOffset]::Parse([string]$manifest.post_build_native_finished_at_utc).UtcDateTime
            if ($nativeDeadline -le $nativeStarted -or
                [Math]::Abs(($nativeDeadline - $nativeStarted).TotalSeconds - [int]$Request.BudgetSeconds.postBuildNativeSeconds) -gt 1 -or
                $nativeFinished -gt $nativeDeadline) {
                $receipt.Classification = 'SCENARIO_CAPTURE_FAILURE'
                $receipt.Error = 'Native interactions and shutdown did not finish within the declared post-build deadline.'
                return $receipt
            }
            $expectedCaptureIndex = 1
            foreach ($capture in @($manifest.capture.captures)) {
                if ([string]$capture.status -cne 'PASS' -or [int]$capture.capture_index -ne $expectedCaptureIndex -or
                    [int]$capture.capture_count -ne [int]$Request.CaptureCount -or
                    [string]$capture.fixture_manifest_sha256 -cne [string]$manifest.fixture_manifest_sha256 -or
                    [int]$capture.process_pid -ne [int]$manifest.executable.pid -or
                    [string]$capture.executable_sha256 -cne [string]$manifest.executable.sha256) {
                    $receipt.Classification = 'RECEIPT_MISMATCH'
                    $receipt.Error = 'Native capture records have a mismatched index, count, fixture, executable, or client binding.'
                    return $receipt
                }
                $screenshotPath = [IO.Path]::GetFullPath((Join-Path $evidenceRoot ([string]$capture.screenshot).Replace('/', '\')))
                $captureReceiptPath = [IO.Path]::GetFullPath((Join-Path $evidenceRoot ([string]$capture.receipt).Replace('/', '\')))
                if (!(Test-WorkflowPathWithin -Candidate $screenshotPath -Directory $evidenceRoot) -or
                    !(Test-WorkflowPathWithin -Candidate $captureReceiptPath -Directory $evidenceRoot) -or
                    !(Test-Path -LiteralPath $screenshotPath -PathType Leaf) -or
                    !(Test-Path -LiteralPath $captureReceiptPath -PathType Leaf)) {
                    $receipt.Classification = 'RECEIPT_MISMATCH'
                    $receipt.Error = 'A native screenshot or capture receipt is missing or escapes the linked run directory.'
                    return $receipt
                }
                $imageHash = (Get-FileHash -LiteralPath $screenshotPath -Algorithm SHA256 -ErrorAction Stop).Hash.ToLowerInvariant()
                if ($imageHash -cne [string]$capture.screenshot_sha256) {
                    $receipt.Classification = 'RECEIPT_MISMATCH'
                    $receipt.Error = 'A linked native screenshot hash does not match the Phase 2 scenario receipt.'
                    return $receipt
                }
                $nativeCapture = Get-Content -LiteralPath $captureReceiptPath -Raw -ErrorAction Stop | ConvertFrom-Json -AsHashtable -ErrorAction Stop
                if ([string]$nativeCapture.status -cne 'saved' -or [string]$nativeCapture.trigger -cne 'f11' -or
                    [string]$nativeCapture.run_id -cne $runId -or [string]$nativeCapture.request_id -cne 'phase2-settings-native' -or
                    [string]$nativeCapture.target -cne 'ROOT' -or [int]$nativeCapture.capture_index -ne $expectedCaptureIndex -or
                    [int]$nativeCapture.capture_count -ne [int]$Request.CaptureCount -or
                    [string]$nativeCapture.saved.png_sha256 -cne $imageHash -or
                    [int]$nativeCapture.process.pid -ne [int]$manifest.executable.pid -or
                    [string]$nativeCapture.executable.sha256 -cne [string]$manifest.executable.sha256 -or
                    [string]$nativeCapture.fixture_manifest_sha256 -cne [string]$manifest.fixture_manifest_sha256 -or
                    [string]$nativeCapture.build_receipt_identity -cne [string]$manifest.build_receipt_identity -or
                    [int64]$capture.logic_frame_id -le 0 -or [int64]$capture.ui_pass_id -le 0 -or
                    [int64]$nativeCapture.captured.logic_frame_id -ne [int64]$capture.logic_frame_id -or
                    [int64]$nativeCapture.captured.ui_pass_id -ne [int64]$capture.ui_pass_id) {
                    $receipt.Classification = 'RECEIPT_MISMATCH'
                    $receipt.Error = 'A native ROOT/F11 receipt does not bind to its screenshot, frame, pass, or capture index.'
                    return $receipt
                }
                $expectedCaptureIndex++
            }
            $receipt.Classification = 'ACTUAL_NATIVE_PASS'
            return $receipt
        }

        if ([string]$manifest.phase -in @('fixture_preparation', 'fixture_tests', 'fixture_generation', 'app_build')) {
            $receipt.Classification = 'BUILD_FAILURE'
        }
        elseif ($cleanupUncertain -or [string]$lifecycle.post_cleanup_state -in @('cleanup_uncertain', 'root_still_alive', 'owned_tree_still_alive')) {
            $receipt.Classification = 'CLEANUP_UNCERTAINTY'
        }
        else { $receipt.Classification = 'SCENARIO_CAPTURE_FAILURE' }
        $receipt.Error = [string]$manifest.failure
        return $receipt
    }
    catch {
        $receipt.Error = $_.Exception.Message
        return $receipt
    }
}

function Export-WorkflowRunRecording {
    param(
        [Parameter(Mandatory)]$Request,
        [Parameter(Mandatory)][string]$RunDirectory,
        [Parameter(Mandatory)]$RunMeasurement,
        [Parameter(Mandatory)]$RecordingContext,
        [Parameter(Mandatory)][System.Collections.Generic.List[object]]$Phases,
        [scriptblock]$ProcessInvoker
    )
    $recording = [pscustomobject][ordered]@{
        Status = 'FAIL'
        RunId = $null
        ArchivePath = $null
        ArchiveSha256 = $null
        ArchiveByteLength = $null
        NativeStaging = $null
        PublicationVerified = $false
        Error = $null
        ConfigPath = [string]$RecordingContext.ConfigPath
        ConfigSha256 = [string]$RecordingContext.ConfigSha256
    }
    $listCommand = New-WorkflowNextestStoreCommand -CargoExecutable $Request.CargoExecutable -Operation list
    try {
        $listMeasurement = Invoke-WorkflowCommand -Request $Request -EvidenceDirectory $RunDirectory -Name 'record-store-list' -Stage 'provenance' `
            -TimeoutSeconds 20 -Command $listCommand -EnvironmentOverrides $RecordingContext.EnvironmentOverrides -ProcessInvoker $ProcessInvoker
        $Phases.Add((Get-WorkflowPhaseSummary -Name 'record-store-list' -Command $listCommand -Measurement $listMeasurement))
        if (!$listMeasurement.Succeeded) { throw "Nextest recording store listing failed ($($listMeasurement.ExecutionReason), exit $($listMeasurement.ExitCode))." }
        $runId = Get-WorkflowRecordedRunId -RunStdoutPath ([string]$RunMeasurement.StdoutPath) `
            -RunStderrPath ([string]$RunMeasurement.StderrPath) -StoreListPath ([string]$listMeasurement.StdoutPath)
        $archivePath = Join-Path $RunDirectory 'nextest-recording.zip'
        $stagingArchivePath = Join-Path ([string]$RecordingContext.StateDirectory) 'recording-export.zip'
        $exportCommand = New-WorkflowNextestStoreCommand -CargoExecutable $Request.CargoExecutable -Operation export -RunId $runId -ArchivePath $stagingArchivePath
        $exportMeasurement = Invoke-WorkflowCommand -Request $Request -EvidenceDirectory $RunDirectory -Name 'record-store-export' -Stage 'provenance' `
            -TimeoutSeconds 30 -Command $exportCommand -EnvironmentOverrides $RecordingContext.EnvironmentOverrides -ProcessInvoker $ProcessInvoker
        $Phases.Add((Get-WorkflowPhaseSummary -Name 'record-store-export' -Command $exportCommand -Measurement $exportMeasurement))
        if (!$exportMeasurement.Succeeded) { throw "Nextest portable recording export failed ($($exportMeasurement.ExecutionReason), exit $($exportMeasurement.ExitCode))." }
        $copy = Copy-WorkflowArchive -SourcePath $stagingArchivePath -DestinationPath $archivePath
        $recording.RunId = $runId
        $recording.ArchivePath = $copy.Destination.Path
        $recording.ArchiveSha256 = $copy.Destination.Sha256
        $recording.ArchiveByteLength = $copy.Destination.ByteLength
        $recording.NativeStaging = [pscustomobject][ordered]@{
            PathScope = 'private Nextest state'
            FileName = [IO.Path]::GetFileName($stagingArchivePath)
            Sha256 = $copy.Source.Sha256
            ByteLength = $copy.Source.ByteLength
            CleanupVerified = $false
        }
        $recording.PublicationVerified = [bool]$copy.ContentMatches
        $recording.Status = 'PASS'
    }
    catch { $recording.Error = $_.Exception.Message }
    return $recording
}

function Invoke-WorkflowAction {
    param(
        [Parameter(Mandatory)]$Request,
        [string]$EvidenceDirectory,
        [scriptblock]$ProcessInvoker
    )
    if ($Request.Action -eq 'Preview') {
        if ($Request.Status -ne 'RESOLVED') {
            return [pscustomobject][ordered]@{ Status = [string]$Request.Status; ExitCode = 3; Error = [string]$Request.ResolutionError; Request = $Request; EvidenceDirectory = $null }
        }
        return [pscustomobject][ordered]@{ Status = 'PREVIEW'; ExitCode = 0; Error = $null; Request = $Request; EvidenceDirectory = $null }
    }

    $runDirectory = New-WorkflowEvidenceDirectory -EvidenceDirectory $EvidenceDirectory
    $phases = [System.Collections.Generic.List[object]]::new()
    $result = [pscustomobject][ordered]@{
        SchemaVersion = 1
        Status = 'RUNNING'
        ExitCode = 1
        Action = [string]$Request.Action
        Mode = [string]$Request.Mode
        SelectionName = $Request.SelectionName
        Request = $Request
        Phases = @()
        Error = $null
        DiscoveryReceiptPath = $null
        Discovery = $null
        SelectedIdentity = $Request.SelectedIdentity
        Results = $null
        ResultsPath = $null
        Classification = $null
        Smoke = $null
        SmokeReceiptPath = $null
        RecordingError = $null
        Recording = $null
        RecordingContext = $null
        RecordingStorageCleanupVerified = $null
        RecordingStorageCleanupError = $null
        ReplaySource = $Request.ReplaySource
        EvidenceDirectory = $runDirectory
        ReceiptPath = $null
    }
    $failureSeen = $false
    $firstFailure = $null

    try {
        if ($Request.WorkflowKind -ceq 'jump-smoke' -and $Request.Action -eq 'Run') {
            $linkPath = Join-Path $runDirectory 'jump-run.json'
            $Request.JumpReceiptPath = $linkPath
            $phase = $Request.Phases[0]
            $phase.Command = New-WorkflowSmokeCommand -Selection $Request.SmokeSelection -ReceiptPath $linkPath
            $measurement = Invoke-WorkflowCommand -Request $Request -EvidenceDirectory $runDirectory -Name 'jump-smoke-run' `
                -Stage 'native' -TimeoutSeconds ([int]$Request.BudgetSeconds.overallSeconds) -Command $phase.Command -ProcessInvoker $ProcessInvoker
            $phases.Add((Get-WorkflowPhaseSummary -Name 'jump-smoke-run' -Command $phase.Command -Measurement $measurement))
            $result.SmokeReceiptPath = $linkPath
            $linkedReceipt = Test-WorkflowSmokeReceipt -Path $linkPath -Request $Request
            $classification = [string]$linkedReceipt.Classification
            if (!$measurement.CleanupVerified -or !$measurement.OutputDrainVerified -or @($measurement.CleanupErrors).Count -gt 0 -or
                @($measurement.RecordingErrors).Count -gt 0) {
                $classification = 'CLEANUP_UNCERTAINTY'
            }
            $result.Classification = $classification
            $result.Smoke = [pscustomobject][ordered]@{
                SelectionName = [string]$Request.SmokeSelectionName
                EvidenceKind = [string]$Request.EvidenceKind
                Classification = $classification
                NativeStatus = if ($linkedReceipt.Link) { [string]$linkedReceipt.Link.native_status } else { 'UNKNOWN' }
                CaptureCount = if ($linkedReceipt.Link) { [int]$linkedReceipt.Link.capture_count } else { $null }
                LinkReceiptPath = if ($linkedReceipt.Valid) { $linkPath } else { $null }
                HarnessEvidenceRoot = if ($linkedReceipt.Valid) { [string]$linkedReceipt.EvidenceRoot } else { $null }
                ManifestPath = if ($linkedReceipt.Link) { [string]$linkedReceipt.Link.manifest_path } else { $null }
                ScenariosPath = if ($linkedReceipt.Link) { [string]$linkedReceipt.Link.scenarios_path } else { $null }
                LifecyclePath = if ($linkedReceipt.Link) { [string]$linkedReceipt.Link.lifecycle_path } else { $null }
                Error = if ($linkedReceipt.Error) { [string]$linkedReceipt.Error } else { $null }
            }
            if ($measurement.Succeeded -and $linkedReceipt.Valid -and $classification -in @('FIXTURE_PASS', 'ACTUAL_NATIVE_PASS')) {
                $result.Status = 'PASS'
                $result.ExitCode = 0
            }
            elseif ($measurement.Succeeded -and $linkedReceipt.Valid -and $classification -ceq 'ENVIRONMENT_NATIVE_NOT_RUN') {
                $result.Status = 'NOT_RUN_ENVIRONMENT'
                $result.ExitCode = 3
                $result.Error = 'Fixture validation completed, but Windows reported no interactive session; native Settings acceptance remains not run.'
            }
            else {
                $result.Status = 'FAIL'
                $result.ExitCode = Get-WorkflowFailureExitCode $measurement
                if ($result.ExitCode -eq 0) { $result.ExitCode = 1 }
                $result.Error = if ($linkedReceipt.Error) { [string]$linkedReceipt.Error } else {
                    "Jump Mode smoke did not produce a valid matching receipt ($($measurement.ExecutionReason), exit $($measurement.ExitCode))."
                }
            }
        }
        elseif ($Request.Action -eq 'Run' -and $Request.ModeKind -eq 'build') {
            $phase = $Request.Phases[0]
            $measurement = Invoke-WorkflowCommand -Request $Request -EvidenceDirectory $runDirectory -Name $phase.Name -Stage $phase.Stage `
                -TimeoutSeconds $phase.TimeoutSeconds -Command $phase.Command -ProcessInvoker $ProcessInvoker
            $summary = Get-WorkflowPhaseSummary -Name $phase.Name -Command $phase.Command -Measurement $measurement
            $phases.Add($summary)
            if (!$measurement.Succeeded) {
                $firstFailure = "Build phase '$($phase.Name)' failed ($($measurement.ExecutionReason), exit $($measurement.ExitCode))."
                $result.ExitCode = Get-WorkflowFailureExitCode $measurement
                throw $firstFailure
            }
            $result.Status = 'PASS'
            $result.ExitCode = 0
        }
        elseif ($Request.Action -eq 'Discover' -or ($Request.Action -in @('Run', 'Replay') -and $Request.Mode -eq 'fast-dev-tests')) {
            $versionCommand = if ($Request.Action -eq 'Discover') { [pscustomobject]@{ Executable = $Request.CargoExecutable; ArgumentList = @('nextest', '--version'); WorkingDirectory = $script:WorkflowRoot } } else { $Request.Phases[0].Command }
            $versionMeasurement = Invoke-WorkflowCommand -Request $Request -EvidenceDirectory $runDirectory -Name 'nextest-version' -Stage 'provenance' `
                -TimeoutSeconds 20 -Command $versionCommand -ProcessInvoker $ProcessInvoker
            $versionSummary = Get-WorkflowPhaseSummary -Name 'nextest-version' -Command $versionCommand -Measurement $versionMeasurement
            $phases.Add($versionSummary)
            if (!$versionMeasurement.Succeeded) {
                $firstFailure = "Cargo Nextest capability check failed ($($versionMeasurement.ExecutionReason), exit $($versionMeasurement.ExitCode))."
                $result.ExitCode = Get-WorkflowFailureExitCode $versionMeasurement
                throw $firstFailure
            }

            $prepareCommand = if ($Request.Action -eq 'Discover') { $Request.PrepareCommand } else { $Request.PrepareCommand }
            $prepareTimeout = [int]$Request.BudgetSeconds.preparationSeconds
            $prepareMeasurement = Invoke-WorkflowCommand -Request $Request -EvidenceDirectory $runDirectory -Name 'target-prepare' -Stage 'compile' `
                -TimeoutSeconds $prepareTimeout -Command $prepareCommand -ProcessInvoker $ProcessInvoker
            $prepareSummary = Get-WorkflowPhaseSummary -Name 'target-prepare' -Command $prepareCommand -Measurement $prepareMeasurement
            $phases.Add($prepareSummary)
            if (!$prepareMeasurement.Succeeded) {
                $firstFailure = "Target preparation failed ($($prepareMeasurement.ExecutionReason), exit $($prepareMeasurement.ExitCode))."
                $result.ExitCode = Get-WorkflowFailureExitCode $prepareMeasurement
                throw $firstFailure
            }
            $metadataPath = [string]$prepareMeasurement.StdoutPath
            $null = Assert-WorkflowBinaryMetadata -Path $metadataPath

            if ($Request.Action -eq 'Discover') {
                $metadataHash = (Get-FileHash -LiteralPath $metadataPath -Algorithm SHA256 -ErrorAction Stop).Hash.ToLowerInvariant()
                $discoverySelections = [System.Collections.Generic.List[object]]::new()
                foreach ($inventoryTemplate in $Request.InventoryTemplates) {
                    $selection = @($Request.Selections | Where-Object { [string]$_.name -ceq [string]$inventoryTemplate.SelectionName })[0]
                    $command = New-WorkflowPhaseTemplate -Preset 'fast-dev-tests' -Phase 'inventory' -Selection $selection `
                        -BinariesMetadataPath $metadataPath -FilterExpression ([string]$inventoryTemplate.FilterExpression)
                    $measurement = Invoke-WorkflowCommand -Request $Request -EvidenceDirectory $runDirectory -Name ("inventory-{0}" -f $selection.name) `
                        -Stage 'provenance' -TimeoutSeconds ([int]$selection.budgets.inventorySeconds) -Command $command -ProcessInvoker $ProcessInvoker
                    $summary = Get-WorkflowPhaseSummary -Name ("inventory-{0}" -f $selection.name) -Command $command -Measurement $measurement
                    $phases.Add($summary)
                    $selectionRecord = [pscustomobject][ordered]@{
                        Name = [string]$selection.name
                        FilterExpression = [string]$inventoryTemplate.FilterExpression
                        Status = 'FAIL'
                        SelectedCaseCount = 0
                        SelectedIdentity = $null
                        InventoryMeasurementPath = [string]$measurement.ManifestPath
                        Error = $null
                    }
                    if (!$measurement.Succeeded) {
                        $selectionRecord.Error = "Inventory process failed ($($measurement.ExecutionReason), exit $($measurement.ExitCode))."
                        if (!$failureSeen) { $failureSeen = $true; $firstFailure = $selectionRecord.Error; $result.ExitCode = Get-WorkflowFailureExitCode $measurement }
                    }
                    else {
                        try {
                            $identity = Get-WorkflowSelectedIdentity -InventoryPath ([string]$measurement.StdoutPath) -Selection $selection
                            $selectionRecord.Status = 'PASS'
                            $selectionRecord.SelectedCaseCount = 1
                            $selectionRecord.SelectedIdentity = $identity
                        }
                        catch {
                            $selectionRecord.Error = $_.Exception.Message
                            if (!$failureSeen) { $failureSeen = $true; $firstFailure = $selectionRecord.Error; $result.ExitCode = 1 }
                        }
                    }
                    $discoverySelections.Add($selectionRecord)
                }
                $finalFingerprint = Get-WorkflowSourceFingerprint
                if ($finalFingerprint -cne $Request.SourceFingerprintSha256 -and !$failureSeen) {
                    $failureSeen = $true
                    $firstFailure = 'Source inputs changed during target discovery; the receipt is not reusable.'
                    $result.ExitCode = 1
                }
                $allSelectionsPass = @($discoverySelections | Where-Object Status -ne 'PASS').Count -eq 0
                $discoveryStatus = if ($allSelectionsPass -and !$failureSeen) { 'PASS' } else { 'FAIL' }
                $toolVersion = if (Test-Path -LiteralPath $versionSummary.StdoutPath -PathType Leaf) { (Get-Content -LiteralPath $versionSummary.StdoutPath -Raw).Trim() } else { $null }
                $discovery = [pscustomobject][ordered]@{
                    schemaVersion = 1
                    status = $discoveryStatus
                    targetKind = [string]$Request.TargetKind
                    targetName = [string]$Request.TargetName
                    package = [string]$Request.Package
                    mode = [string]$Request.Mode
                    compilerProfile = [string]$Request.CompilerProfile
                    runnerProfile = [string]$Request.RunnerProfile
                    features = $Request.Features
                    catalogSha256 = [string]$Request.CatalogSha256
                    sourceFingerprintSha256 = [string]$Request.SourceFingerprintSha256
                    cargoExecutable = [string]$Request.CargoExecutable
                    cargoExecutableSha256 = (Get-FileHash -LiteralPath $Request.CargoExecutable -Algorithm SHA256 -ErrorAction Stop).Hash.ToLowerInvariant()
                    nextestTool = New-WorkflowToolReceipt -Tool $Request.NextestTool
                    nextestVersion = $toolVersion
                    preparationMeasurementPath = [string]$prepareMeasurement.ManifestPath
                    binariesMetadataPath = $metadataPath
                    binariesMetadataSha256 = $metadataHash
                    selections = @($discoverySelections)
                    phases = @($phases)
                    error = $firstFailure
                    evidenceDirectory = $runDirectory
                }
                $receiptPath = Join-Path $runDirectory 'discovery.json'
                Write-WorkflowJson -Path $receiptPath -Value $discovery
                $result.DiscoveryReceiptPath = $receiptPath
                $result.Status = $discoveryStatus
                $result.ExitCode = if ($discoveryStatus -eq 'PASS') { 0 } else { if ($result.ExitCode -eq 0) { 1 } else { $result.ExitCode } }
                $result.Error = $firstFailure
                $result.Discovery = $discovery
            }
            else {
                Set-WorkflowRequestMetadataPath -Request $Request -Path $metadataPath
                $inventoryMeasurement = Invoke-WorkflowCommand -Request $Request -EvidenceDirectory $runDirectory -Name 'selected-inventory' -Stage 'provenance' `
                    -TimeoutSeconds ([int]$Request.BudgetSeconds.inventorySeconds) -Command $Request.InventoryCommand -ProcessInvoker $ProcessInvoker
                $inventorySummary = Get-WorkflowPhaseSummary -Name 'selected-inventory' -Command $Request.InventoryCommand -Measurement $inventoryMeasurement
                $phases.Add($inventorySummary)
                if (!$inventoryMeasurement.Succeeded) {
                    $firstFailure = "Selected inventory failed ($($inventoryMeasurement.ExecutionReason), exit $($inventoryMeasurement.ExitCode))."
                    $result.ExitCode = Get-WorkflowFailureExitCode $inventoryMeasurement
                    throw $firstFailure
                }
                $identity = Get-WorkflowSelectedIdentity -InventoryPath ([string]$inventoryMeasurement.StdoutPath) -Selection $Request.Selection
                $Request.SelectedIdentity = $identity
                $runPhase = $Request.Phases[3]
                $recordingContext = New-WorkflowRecordingContext -EvidenceDirectory $runDirectory
                $runFailure = $null
                try {
                    $result.RecordingContext = [pscustomobject][ordered]@{
                        ConfigPath = $recordingContext.ConfigPath
                        ConfigSha256 = $recordingContext.ConfigSha256
                        StateDirectory = $recordingContext.StateDirectory
                        EnvironmentNames = @($recordingContext.EnvironmentOverrides.Keys | Sort-Object)
                    }
                    if ($Request.Action -eq 'Replay') {
                        $replayStaging = Stage-WorkflowReplayArchive -Request $Request -RecordingContext $recordingContext
                        $result.RecordingContext | Add-Member -NotePropertyName ReplayStaging -NotePropertyValue $replayStaging -Force
                    }
                    $result.SelectedIdentity = $identity
                    $runMeasurement = Invoke-WorkflowCommand -Request $Request -EvidenceDirectory $runDirectory -Name 'selected-run' -Stage 'test' `
                        -TimeoutSeconds ([int]$Request.BudgetSeconds.executionSeconds) -Command $Request.RunCommand `
                        -EnvironmentOverrides $recordingContext.EnvironmentOverrides -ProcessInvoker $ProcessInvoker
                    $runSummary = Get-WorkflowPhaseSummary -Name 'selected-run' -Command $Request.RunCommand -Measurement $runMeasurement
                    $phases.Add($runSummary)
                    if (!$runMeasurement.Succeeded) {
                        $runFailure = "Selected test execution failed ($($runMeasurement.ExecutionReason), exit $($runMeasurement.ExitCode))."
                        $result.ExitCode = Get-WorkflowFailureExitCode $runMeasurement
                    }
                    try {
                        $structuredResults = Get-WorkflowStructuredResults -StdoutPath ([string]$runMeasurement.StdoutPath) `
                            -InventoryPath ([string]$inventoryMeasurement.StdoutPath) -SelectedIdentity $identity -Measurement $runMeasurement
                    }
                    catch {
                        $structuredResults = [pscustomobject][ordered]@{
                            SchemaVersion = 1; Format = 'nextest-libtest-json-plus-0.1'; Selected = 1; Passed = 0; Failed = 0; TimedOut = $null
                            ObservedTimedOut = 0; TimeoutCountKnown = $false
                            TimeoutCountReason = 'Structured output could not be reduced; the total timed-out case count is unknown.'
                            Ignored = 0; Filtered = 0; Skipped = 0; Unexecuted = 1; StartedOnly = 0; Unknown = 1; Complete = $false
                            SuiteStartedCount = 0; SuiteTerminalCount = 0; SuiteActiveAtEnd = 0; SuiteTerminalStatuses = @(); SuiteComplete = $false
                            PhaseReliable = $false; PhaseReliabilityErrors = @('Structured result reduction failed.')
                            NativeExitCode = $runMeasurement.ExitCode; StageTimedOut = [bool]$runMeasurement.TimedOut
                            CleanupVerified = [bool]$runMeasurement.CleanupVerified; OutputDrainVerified = [bool]$runMeasurement.OutputDrainVerified
                            ExecutionWallSeconds = $runMeasurement.WallSeconds; TerminalAttemptCount = 0; Cases = @(); SlowCases = @()
                            ConfirmedFailures = @(); ParseErrors = @($_.Exception.Message)
                        }
                    }
                    $result.Results = $structuredResults
                    $resultsPath = Join-Path $runDirectory 'results.json'
                    $result.ResultsPath = $resultsPath
                    try { Write-WorkflowJson -Path $resultsPath -Value $structuredResults }
                    catch {
                        if (!$runFailure) { $runFailure = "Structured results could not be recorded: $($_.Exception.Message)"; $result.ExitCode = 1 }
                        else { $result.RecordingError = "Structured results could not be recorded: $($_.Exception.Message)" }
                    }

                    if (!$structuredResults.Complete -and !$runFailure) {
                        $runFailure = 'Selected test output was partial, malformed, ambiguous, or uncertain; see results.json.'
                        $result.ExitCode = if ($runMeasurement.ExitCode -ne 0) { Get-WorkflowFailureExitCode $runMeasurement } else { 1 }
                    }
                    if ($structuredResults.Failed -gt 0 -or $structuredResults.ObservedTimedOut -gt 0) {
                        if (!$runFailure) { $runFailure = 'Selected test execution recorded a failing or timed-out test.' }
                        if ($result.ExitCode -eq 0) { $result.ExitCode = if ($structuredResults.ObservedTimedOut -gt 0) { 124 } else { 1 } }
                    }

                    $recording = Export-WorkflowRunRecording -Request $Request -RunDirectory $runDirectory -RunMeasurement $runMeasurement `
                        -RecordingContext $recordingContext -Phases $phases -ProcessInvoker $ProcessInvoker
                    $result.Recording = $recording
                    if ($recording.Status -ne 'PASS' -and !$runFailure) {
                        $runFailure = "Test result recording failed: $($recording.Error)"
                        $result.ExitCode = 1
                    }

                    if ($runFailure) {
                        $firstFailure = $runFailure
                        throw $firstFailure
                    }
                    if ((Get-WorkflowSourceFingerprint) -cne $Request.SourceFingerprintSha256) {
                        $firstFailure = 'Source inputs changed during selected execution; provenance is uncertain.'
                        $result.ExitCode = 1
                        throw $firstFailure
                    }
                    $result.Status = 'PASS'
                    $result.ExitCode = 0
                }
                finally {
                    try { Remove-WorkflowRecordingContext -Context $recordingContext }
                    catch {
                        $recordingContext.CleanupVerified = $false
                        $recordingContext.CleanupError = $_.Exception.Message
                    }
                    $result.RecordingStorageCleanupVerified = [bool]$recordingContext.CleanupVerified
                    $result.RecordingStorageCleanupError = $recordingContext.CleanupError
                    if ($result.Recording -and $result.Recording.NativeStaging) {
                        $result.Recording.NativeStaging.CleanupVerified = [bool]$recordingContext.CleanupVerified
                    }
                    if ($Request.ReplaySource -and $Request.ReplaySource.RuntimeRecording) {
                        $Request.ReplaySource.RuntimeRecording.CleanupVerified = [bool]$recordingContext.CleanupVerified
                        if ($result.RecordingContext.ReplayStaging) {
                            $result.RecordingContext.ReplayStaging.CleanupVerified = [bool]$recordingContext.CleanupVerified
                        }
                    }
                    if (!$recordingContext.CleanupVerified -and !$runFailure) {
                        $runFailure = "Private Nextest state cleanup failed: $($recordingContext.CleanupError)"
                        $result.ExitCode = 1
                    }
                }
                if ($runFailure) {
                    $firstFailure = $runFailure
                    throw $firstFailure
                }
            }
        }
        else { throw "Unsupported workflow action/mode combination: $($Request.Action) / $($Request.Mode)." }
    }
    catch {
        if (!$firstFailure) { $firstFailure = $_.Exception.Message }
        if ($result.ExitCode -eq 0) { $result.ExitCode = 1 }
        if ($Request.Action -ne 'Discover' -or !$result.DiscoveryReceiptPath) { $result.Status = 'FAIL' }
        $result.Error = $firstFailure
    }

    $result.Phases = @($phases)
    $resultPath = Join-Path $runDirectory 'workflow.json'
    $result.ReceiptPath = $resultPath
    try { Write-WorkflowJson -Path $resultPath -Value $result }
    catch {
        if ($result.Status -eq 'PASS') { $result.Status = 'FAIL'; $result.ExitCode = 1 }
        if (!$result.Error) { $result.Error = "Workflow receipt could not be written: $($_.Exception.Message)" }
    }
    return $result
}
