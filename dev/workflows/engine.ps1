# Shared resolver and bounded workflow action boundary. Dot-source this file from
# run.ps1 or the focused self-test; child processes always go through measure.ps1.

$script:WorkflowRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..\..') -ErrorAction Stop).ProviderPath
$script:WorkflowCatalogPath = Join-Path $PSScriptRoot 'catalog.json'
$script:WorkflowCargoPath = Join-Path $script:WorkflowRoot 'dev\track_b\cargo.ps1'
$script:WorkflowMeasurePath = Join-Path $script:WorkflowRoot 'dev\track_b\measure.ps1'
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
        if ($Request.Action -eq 'Run' -and $Request.ModeKind -eq 'build') {
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
